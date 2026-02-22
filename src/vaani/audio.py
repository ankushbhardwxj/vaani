"""Mic capture, Voice Activity Detection, gain normalization, and WAV encoding."""

import io
import logging
import threading
from typing import Optional

import numpy as np
import sounddevice as sd
import soundfile as sf

logger = logging.getLogger(__name__)


def list_microphones() -> list[dict]:
    """Return a list of available microphones with their info."""
    devices = sd.query_devices()
    mics = []
    for i, device in enumerate(devices):
        if device['max_input_channels'] > 0:
            mics.append({
                'index': i,
                'name': device['name'],
                'channels': device['max_input_channels'],
                'is_default': i == sd.default.device[0] if isinstance(sd.default.device, tuple) else i == sd.default.device,
            })
    return mics


def get_default_microphone_index() -> int:
    """Return the index of the default microphone."""
    default = sd.query_devices(kind='input')
    # Find index by matching device info
    devices = sd.query_devices()
    for i, device in enumerate(devices):
        if device == default:
            return i
    return 0

# Lazy-loaded VAD model (PyTorch + Silero is ~500MB, load on first use)
_vad_model = None
_vad_utils = None
_vad_lock = threading.Lock()

TARGET_DBFS = -20.0  # Target RMS level for gain normalization


def _load_vad():
    """Lazy-load Silero VAD model on first recording."""
    global _vad_model, _vad_utils
    if _vad_model is not None:
        return

    with _vad_lock:
        if _vad_model is not None:
            return
        import torch

        logger.info("Loading Silero VAD model (first use)...")
        model, utils = torch.hub.load(
            repo_or_dir="snakers4/silero-vad",
            model="silero_vad",
            trust_repo=True,
        )
        _vad_model = model
        _vad_utils = utils
        logger.info("Silero VAD model loaded")


class AudioRecorder:
    """Records audio from a specified microphone into a growing buffer."""

    def __init__(self, sample_rate: int = 16000, device: Optional[int] = None) -> None:
        self.sample_rate = sample_rate
        self.device = device  # None means use default
        self._chunks: list[np.ndarray] = []
        self._stream: Optional[sd.InputStream] = None
        self._recording = threading.Event()

    def start(self) -> None:
        """Start recording audio."""
        self._chunks.clear()
        self._recording.set()
        self._stream = sd.InputStream(
            device=self.device,
            samplerate=self.sample_rate,
            channels=1,
            dtype="float32",
            callback=self._audio_callback,
        )
        self._stream.start()
        device_info = sd.query_devices(self.device)
        device_name = device_info['name'] if isinstance(device_info, dict) else "unknown"
        logger.info("Recording started on device: %s", device_name)

    def _audio_callback(self, indata, frames, time_info, status):
        if status:
            logger.warning("Audio callback status: %s", status)
        if self._recording.is_set():
            self._chunks.append(indata.copy())

    def stop(self) -> np.ndarray:
        """Stop recording and return the raw audio as a 1D float32 array."""
        self._recording.clear()
        if self._stream:
            self._stream.stop()
            self._stream.close()
            self._stream = None
        logger.info("Recording stopped, %d chunks captured", len(self._chunks))

        if not self._chunks:
            return np.array([], dtype=np.float32)

        audio = np.concatenate(self._chunks, axis=0).flatten()
        return audio

    @property
    def current_level(self) -> float:
        """Return RMS level of the most recent audio (0.0–1.0)."""
        if not self._chunks:
            return 0.0
        recent = np.concatenate(self._chunks[-4:])
        rms = float(np.sqrt(np.mean(recent ** 2)))
        return min(rms * 12, 1.0)  # scale so normal speech hits ~0.5–0.8

    def cancel(self) -> None:
        """Cancel recording, discard audio."""
        self._recording.clear()
        if self._stream:
            self._stream.stop()
            self._stream.close()
            self._stream = None
        self._chunks.clear()
        logger.info("Recording cancelled")


def trim_silence(audio: np.ndarray, sample_rate: int = 16000, threshold: float = 0.3) -> np.ndarray:
    """Use Silero VAD to trim silence from audio. Returns trimmed audio."""
    if len(audio) == 0:
        return audio

    _load_vad()

    import torch

    # Silero VAD expects 16kHz mono, chunks of 512 samples
    chunk_size = 512
    speech_chunks = []
    has_speech = False

    _vad_model.reset_states()

    for i in range(0, len(audio) - chunk_size + 1, chunk_size):
        chunk = audio[i : i + chunk_size]
        tensor = torch.from_numpy(chunk).float()
        prob = _vad_model(tensor, sample_rate).item()
        if prob >= threshold:
            has_speech = True
            # Include some context around speech
            start = max(0, i - chunk_size)
            end = min(len(audio), i + chunk_size * 2)
            speech_chunks.append((start, end))

    if not has_speech:
        logger.info("No speech detected by VAD")
        return np.array([], dtype=np.float32)

    # Merge overlapping regions
    merged = [speech_chunks[0]]
    for start, end in speech_chunks[1:]:
        if start <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(merged[-1][1], end))
        else:
            merged.append((start, end))

    # Extract and concatenate speech regions
    parts = [audio[s:e] for s, e in merged]
    trimmed = np.concatenate(parts)

    ratio = len(trimmed) / len(audio)
    logger.info("VAD trimmed audio: %.1f%% speech (%.2fs → %.2fs)",
                ratio * 100, len(audio) / sample_rate, len(trimmed) / sample_rate)
    return trimmed


def normalize_gain(audio: np.ndarray, target_dbfs: float = TARGET_DBFS) -> np.ndarray:
    """RMS gain normalization to target dBFS level. Helps with whisper-level audio."""
    if len(audio) == 0:
        return audio

    rms = np.sqrt(np.mean(audio ** 2))
    if rms < 1e-10:
        logger.warning("Audio is essentially silent, skipping normalization")
        return audio

    current_dbfs = 20 * np.log10(rms)
    gain_db = target_dbfs - current_dbfs
    gain_linear = 10 ** (gain_db / 20)

    normalized = audio * gain_linear

    # Clip to prevent distortion
    normalized = np.clip(normalized, -1.0, 1.0)
    logger.info("Gain normalization: %.1f dBFS → %.1f dBFS (gain: %.1f dB)",
                current_dbfs, target_dbfs, gain_db)
    return normalized


def encode_wav(audio: np.ndarray, sample_rate: int = 16000) -> bytes:
    """Encode float32 PCM audio to WAV bytes."""
    buf = io.BytesIO()
    sf.write(buf, audio, sample_rate, format="WAV", subtype="PCM_16")
    buf.seek(0)
    return buf.read()


def process_audio(
    audio: np.ndarray,
    sample_rate: int = 16000,
    vad_threshold: float = 0.15,
) -> Optional[bytes]:
    """Full audio pipeline: gain normalize → VAD trim → encode WAV.

    Normalization runs first so whisper-level audio is amplified before
    VAD attempts to detect speech.

    Returns WAV bytes, or None if no speech detected.
    """
    normalized = normalize_gain(audio)
    trimmed = trim_silence(normalized, sample_rate, vad_threshold)
    if len(trimmed) == 0:
        return None

    wav_bytes = encode_wav(trimmed, sample_rate)
    logger.info("Audio processed: %d bytes WAV", len(wav_bytes))
    return wav_bytes

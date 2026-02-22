"""Tests for vaani.audio — normalize_gain, encode_wav, trim_silence, AudioRecorder."""

import io
import struct
from unittest.mock import MagicMock, patch

import numpy as np
import pytest
import soundfile as sf


# ---------------------------------------------------------------------------
# normalize_gain
# ---------------------------------------------------------------------------

class TestNormalizeGain:
    def test_amplifies_quiet_audio(self):
        from vaani.audio import normalize_gain
        quiet = np.full(1600, 0.001, dtype=np.float32)
        result = normalize_gain(quiet, target_dbfs=-20.0)
        assert np.sqrt(np.mean(result ** 2)) > np.sqrt(np.mean(quiet ** 2))

    def test_attenuates_loud_audio(self):
        from vaani.audio import normalize_gain
        loud = np.full(1600, 0.9, dtype=np.float32)
        result = normalize_gain(loud, target_dbfs=-20.0)
        assert np.sqrt(np.mean(result ** 2)) < np.sqrt(np.mean(loud ** 2))

    def test_clips_to_valid_range(self):
        from vaani.audio import normalize_gain
        loud = np.full(1600, 0.99, dtype=np.float32)
        result = normalize_gain(loud, target_dbfs=-3.0)
        assert result.min() >= -1.0
        assert result.max() <= 1.0

    def test_empty_returns_empty(self):
        from vaani.audio import normalize_gain
        empty = np.array([], dtype=np.float32)
        result = normalize_gain(empty)
        assert len(result) == 0

    def test_silent_audio_returned_unchanged(self):
        from vaani.audio import normalize_gain
        silent = np.zeros(1600, dtype=np.float32)
        result = normalize_gain(silent)
        np.testing.assert_array_equal(result, silent)


# ---------------------------------------------------------------------------
# encode_wav
# ---------------------------------------------------------------------------

class TestEncodeWav:
    def test_produces_valid_wav_bytes(self):
        from vaani.audio import encode_wav
        audio = np.random.randn(16000).astype(np.float32) * 0.5
        wav = encode_wav(audio, sample_rate=16000)
        assert wav[:4] == b"RIFF"

    def test_round_trip(self):
        from vaani.audio import encode_wav
        audio = np.random.randn(16000).astype(np.float32) * 0.3
        wav = encode_wav(audio, sample_rate=16000)
        data, sr = sf.read(io.BytesIO(wav), dtype="float32")
        assert sr == 16000
        assert len(data) == len(audio)

    def test_correct_sample_rate(self):
        from vaani.audio import encode_wav
        audio = np.random.randn(8000).astype(np.float32) * 0.5
        wav = encode_wav(audio, sample_rate=8000)
        _, sr = sf.read(io.BytesIO(wav), dtype="float32")
        assert sr == 8000

    def test_empty_audio(self):
        from vaani.audio import encode_wav
        empty = np.array([], dtype=np.float32)
        wav = encode_wav(empty, sample_rate=16000)
        assert wav[:4] == b"RIFF"


# ---------------------------------------------------------------------------
# trim_silence (mock VAD model)
# ---------------------------------------------------------------------------

class TestTrimSilence:
    @pytest.fixture(autouse=True)
    def _mock_vad(self, monkeypatch):
        """Patch VAD model to return predetermined probabilities."""
        import vaani.audio

        fake_model = MagicMock()
        # Default: return high probability for all chunks
        fake_model.return_value.item.return_value = 0.9
        fake_model.reset_states = MagicMock()

        monkeypatch.setattr("vaani.audio._vad_model", fake_model)
        monkeypatch.setattr("vaani.audio._vad_utils", MagicMock())
        self.fake_model = fake_model

    def test_speech_detected_returns_nonempty(self):
        from vaani.audio import trim_silence
        audio = np.random.randn(16000).astype(np.float32)
        result = trim_silence(audio, sample_rate=16000, threshold=0.3)
        assert len(result) > 0

    def test_no_speech_returns_empty(self):
        from vaani.audio import trim_silence
        self.fake_model.return_value.item.return_value = 0.0
        audio = np.random.randn(16000).astype(np.float32)
        result = trim_silence(audio, sample_rate=16000, threshold=0.3)
        assert len(result) == 0

    def test_empty_input(self):
        from vaani.audio import trim_silence
        result = trim_silence(np.array([], dtype=np.float32))
        assert len(result) == 0


# ---------------------------------------------------------------------------
# process_audio
# ---------------------------------------------------------------------------

class TestProcessAudio:
    @pytest.fixture(autouse=True)
    def _mock_vad(self, monkeypatch):
        import vaani.audio
        fake_model = MagicMock()
        fake_model.return_value.item.return_value = 0.0  # no speech
        fake_model.reset_states = MagicMock()
        monkeypatch.setattr("vaani.audio._vad_model", fake_model)
        monkeypatch.setattr("vaani.audio._vad_utils", MagicMock())
        self.fake_model = fake_model

    def test_returns_none_when_no_speech(self):
        from vaani.audio import process_audio
        audio = np.random.randn(16000).astype(np.float32)
        assert process_audio(audio, sample_rate=16000) is None

    def test_returns_wav_bytes_when_speech(self):
        from vaani.audio import process_audio
        self.fake_model.return_value.item.return_value = 0.9
        audio = np.random.randn(16000).astype(np.float32) * 0.1
        result = process_audio(audio, sample_rate=16000, vad_threshold=0.3)
        assert result is not None
        assert result[:4] == b"RIFF"


# ---------------------------------------------------------------------------
# AudioRecorder._audio_callback
# ---------------------------------------------------------------------------

class TestAudioRecorderCallback:
    def test_buffer_accumulation(self):
        from vaani.audio import AudioRecorder
        rec = AudioRecorder(sample_rate=16000)
        rec._recording.set()  # simulate recording state

        chunk = np.random.randn(512, 1).astype(np.float32)
        rec._audio_callback(chunk, 512, None, None)
        rec._audio_callback(chunk, 512, None, None)

        assert len(rec._chunks) == 2

    def test_current_level(self):
        from vaani.audio import AudioRecorder
        rec = AudioRecorder(sample_rate=16000)
        rec._recording.set()

        chunk = np.full((512, 1), 0.5, dtype=np.float32)
        rec._audio_callback(chunk, 512, None, None)

        level = rec.current_level
        assert 0.0 < level <= 1.0

    def test_no_chunks_gives_zero_level(self):
        from vaani.audio import AudioRecorder
        rec = AudioRecorder(sample_rate=16000)
        assert rec.current_level == 0.0

    def test_not_recording_skips_chunk(self):
        from vaani.audio import AudioRecorder
        rec = AudioRecorder(sample_rate=16000)
        # _recording is NOT set
        chunk = np.random.randn(512, 1).astype(np.float32)
        rec._audio_callback(chunk, 512, None, None)
        assert len(rec._chunks) == 0

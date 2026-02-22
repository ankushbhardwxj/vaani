"""OpenAI Whisper API transcription."""

import io
import logging
import time

from openai import OpenAI

from vaani.config import get_openai_key

logger = logging.getLogger(__name__)


def transcribe(wav_bytes: bytes, model: str = "gpt-4o-mini-transcribe") -> str:
    """Send WAV audio to OpenAI Whisper API and return transcribed text.

    Args:
        wav_bytes: WAV-encoded audio bytes.
        model: OpenAI STT model to use.

    Returns:
        Transcribed text string.

    Raises:
        RuntimeError: If API key is missing or API call fails.
    """
    api_key = get_openai_key()
    if not api_key:
        raise RuntimeError(
            "OpenAI API key not found. Run 'vaani setup' to configure."
        )

    client = OpenAI(api_key=api_key)

    audio_file = io.BytesIO(wav_bytes)
    audio_file.name = "recording.wav"

    start = time.monotonic()
    response = client.audio.transcriptions.create(
        model=model,
        file=audio_file,
    )
    elapsed = time.monotonic() - start

    text = response.text.strip()
    logger.info("Transcription complete in %.2fs (%d chars)", elapsed, len(text))
    return text

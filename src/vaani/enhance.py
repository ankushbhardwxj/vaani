"""Claude Haiku streaming text enhancement."""

import logging
import time
from pathlib import Path
from typing import Optional

import anthropic

from vaani.config import VAANI_DIR, get_anthropic_key

logger = logging.getLogger(__name__)

# Prompt directories: project prompts (version-controlled) and user overrides
_PROJECT_PROMPTS = Path(__file__).parent.parent.parent / "prompts"
_USER_PROMPTS = VAANI_DIR / "prompts"


def _load_prompt_file(relative_path: str) -> Optional[str]:
    """Load a prompt file, preferring user override over project default."""
    user_path = _USER_PROMPTS / relative_path
    if user_path.exists():
        return user_path.read_text().strip()

    project_path = _PROJECT_PROMPTS / relative_path
    if project_path.exists():
        return project_path.read_text().strip()

    return None


def _build_system_prompt(mode: str) -> str:
    """Assemble system prompt from: system.txt + context.txt + modes/{mode}.txt."""
    parts = []

    system = _load_prompt_file("system.txt")
    if system:
        parts.append(system)

    context = _load_prompt_file("context.txt")
    if context:
        parts.append(context)

    mode_prompt = _load_prompt_file(f"modes/{mode}.txt")
    if mode_prompt:
        parts.append(mode_prompt)

    if not parts:
        # Fallback if no prompt files exist
        parts.append(
            "You enhance spoken transcriptions into polished written text. "
            "Fix grammar, remove filler words, and improve clarity while "
            "preserving the speaker's meaning and intent."
        )

    return "\n\n".join(parts)


def enhance(
    transcription: str,
    mode: str = "cleanup",
    model: str = "claude-haiku-4-5-20251001",
) -> str:
    """Enhance transcribed text using Claude Haiku with streaming.

    Args:
        transcription: Raw transcribed text from Whisper.
        mode: Enhancement mode (cleanup, professional, casual, bullets).
        model: Anthropic model to use.

    Returns:
        Enhanced text string.

    Raises:
        RuntimeError: If API key is missing or API call fails.
    """
    api_key = get_anthropic_key()
    if not api_key:
        raise RuntimeError(
            "Anthropic API key not found. Run 'vaani setup' to configure."
        )

    system_prompt = _build_system_prompt(mode)
    client = anthropic.Anthropic(api_key=api_key)

    start = time.monotonic()
    result_parts = []

    with client.messages.stream(
        model=model,
        max_tokens=4096,
        system=system_prompt,
        messages=[
            {
                "role": "user",
                "content": transcription,
            }
        ],
    ) as stream:
        for text in stream.text_stream:
            result_parts.append(text)

    enhanced = "".join(result_parts).strip()
    elapsed = time.monotonic() - start
    logger.info(
        "Enhancement complete in %.2fs (mode=%s, %d → %d chars)",
        elapsed, mode, len(transcription), len(enhanced),
    )
    return enhanced

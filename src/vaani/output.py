"""Clipboard-safe paste: save clipboard → copy text → Cmd+V → restore clipboard."""

import logging
import subprocess
import threading
import time
from typing import Optional

from pynput.keyboard import Controller, Key

logger = logging.getLogger(__name__)

# Lazy-loaded spaCy NER model
_nlp = None
_nlp_lock = threading.Lock()


def _load_nlp():
    """Lazy-load spaCy NER model on first use."""
    global _nlp
    if _nlp is not None:
        return

    with _nlp_lock:
        if _nlp is not None:
            return
        try:
            import spacy
            logger.info("Loading spaCy NER model...")
            _nlp = spacy.load("en_core_web_sm")
            logger.info("spaCy NER model loaded")
        except Exception as e:
            logger.warning("Failed to load spaCy NER: %s", e)
            _nlp = None


def _format_names_with_at(text: str) -> str:
    """Use NER to identify person names and format with @ prefix."""
    try:
        _load_nlp()
        if _nlp is None:
            return text

        doc = _nlp(text)
        formatted = text

        # Process entities in reverse order to maintain string positions
        for ent in reversed(doc.ents):
            if ent.label_ == "PERSON":
                name = ent.text.strip()
                # Skip if already formatted with @
                if not name.startswith("@"):
                    # Replace the name with @-prefixed version
                    formatted = formatted[:ent.start_char] + "@" + name + formatted[ent.end_char:]

        return formatted
    except Exception as e:
        logger.debug("NER formatting failed: %s", e)
        return text


def _get_clipboard() -> str:
    """Get current clipboard contents via pbpaste."""
    try:
        result = subprocess.run(
            ["pbpaste"], capture_output=True, text=True, timeout=5
        )
        return result.stdout
    except Exception:
        logger.warning("Failed to read clipboard")
        return ""


def _set_clipboard(text: str) -> None:
    """Set clipboard contents via pbcopy."""
    try:
        subprocess.run(
            ["pbcopy"],
            input=text,
            text=True,
            timeout=5,
            check=True,
        )
    except Exception:
        logger.warning("Failed to set clipboard")


def paste_text(text: str, restore_delay_ms: int = 100) -> None:
    """Paste text at the current cursor position, preserving the clipboard.

    1. Format names with NER (@ prefix for person names)
    2. Save current clipboard
    3. Copy enhanced text to clipboard
    4. Simulate Cmd+V
    5. Wait for paste to be consumed
    6. Restore original clipboard
    """
    # Format names with @ prefix using NER
    text = _format_names_with_at(text)

    original = _get_clipboard()

    _set_clipboard(text)

    # Small delay to ensure clipboard is set
    time.sleep(0.05)

    # Simulate Cmd+V paste using pynput (consistent with the hotkey listener)
    _keyboard = Controller()
    with _keyboard.pressed(Key.cmd):
        _keyboard.tap('v')

    # Wait for the paste event to be consumed by the target app
    time.sleep(restore_delay_ms / 1000.0)

    # Restore original clipboard
    _set_clipboard(original)
    logger.info("Text pasted at cursor (%d chars), clipboard restored", len(text))

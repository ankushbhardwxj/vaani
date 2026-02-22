"""Clipboard-safe paste: save clipboard → copy text → Cmd+V → restore clipboard."""

import logging
import subprocess
import time

from pynput.keyboard import Controller, Key

logger = logging.getLogger(__name__)


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

    1. Save current clipboard
    2. Copy enhanced text to clipboard
    3. Simulate Cmd+V
    4. Wait for paste to be consumed
    5. Restore original clipboard
    """
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

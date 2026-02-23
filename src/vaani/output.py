"""Clipboard-safe paste: save clipboard → copy text → Cmd+V → restore clipboard."""

import logging
import subprocess
import threading
import time
from pynput.keyboard import Controller, Key

logger = logging.getLogger(__name__)

# Lazy-loaded spaCy NER model
_nlp = None
_nlp_lock = threading.Lock()


def _load_nlp():
    """Load spaCy NER model. Should be called during prewarm, not during recording."""
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


def is_nlp_loaded() -> bool:
    """Return True if the spaCy NER model has been loaded (or failed permanently)."""
    return _nlp is not None


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


def _do_cmd_v() -> None:
    """Press Cmd+V using pynput. Must be called on the main thread when NSApp is active."""
    keyboard = Controller()
    with keyboard.pressed(Key.cmd):
        keyboard.tap('v')


def _simulate_cmd_v(restore_delay_ms: int) -> None:
    """Simulate Cmd+V, dispatching to the main thread if rumps/NSApp owns it.

    macOS TSM (Text Services Manager) APIs used by pynput crash with
    _dispatch_assert_queue_fail when called off the main thread while
    an NSApplication run loop is active.
    """
    try:
        from AppKit import NSApp
        from PyObjCTools import AppHelper

        app = NSApp()
        if app is not None and app.isRunning():
            done = threading.Event()

            def _on_main():
                try:
                    _do_cmd_v()
                    time.sleep(restore_delay_ms / 1000.0)
                finally:
                    done.set()

            AppHelper.callAfter(_on_main)
            if not done.wait(timeout=10):
                logger.error("Timed out dispatching Cmd+V to main thread")
            return
    except ImportError:
        pass

    _do_cmd_v()
    time.sleep(restore_delay_ms / 1000.0)


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

    _simulate_cmd_v(restore_delay_ms)

    # Restore original clipboard
    _set_clipboard(original)
    logger.info("Text pasted at cursor (%d chars), clipboard restored", len(text))

"""Global hotkey listener using pynput."""

import logging
import threading
from typing import Callable, Optional

from pynput import keyboard

logger = logging.getLogger(__name__)

# Default hotkey: Cmd+Shift+;
DEFAULT_HOTKEY = "<cmd>+<shift>+;"


class HotkeyListener:
    """Detects global hotkey press/release and Escape for cancellation.

    The hotkey works in press-and-hold mode:
    - Press hotkey combo → on_press callback
    - Release hotkey combo → on_release callback
    - Press Escape during recording → on_cancel callback
    """

    def __init__(
        self,
        hotkey: str = DEFAULT_HOTKEY,
        on_press: Optional[Callable] = None,
        on_release: Optional[Callable] = None,
        on_cancel: Optional[Callable] = None,
    ) -> None:
        self.hotkey = hotkey
        self.on_press = on_press
        self.on_release = on_release
        self.on_cancel = on_cancel
        self._listener: Optional[keyboard.Listener] = None
        self._hotkey_pressed = False
        self._current_keys: set = set()
        self._hotkey_keys = self._parse_hotkey(hotkey)

    # Map of special key names to pynput Key objects
    _SPECIAL_KEYS = {
        "<cmd>": keyboard.Key.cmd, "cmd": keyboard.Key.cmd,
        "<shift>": keyboard.Key.shift, "shift": keyboard.Key.shift,
        "<ctrl>": keyboard.Key.ctrl, "ctrl": keyboard.Key.ctrl,
        "<alt>": keyboard.Key.alt, "alt": keyboard.Key.alt,
        "space": keyboard.Key.space, "<space>": keyboard.Key.space,
        "esc": keyboard.Key.esc, "<esc>": keyboard.Key.esc,
        "tab": keyboard.Key.tab, "<tab>": keyboard.Key.tab,
        "enter": keyboard.Key.enter, "<enter>": keyboard.Key.enter,
    }

    def _parse_hotkey(self, hotkey_str: str) -> set:
        """Parse hotkey string like '<cmd>+<shift>+;' into a set of keys."""
        parts = hotkey_str.lower().split("+")
        keys = set()
        for part in parts:
            part = part.strip()
            if part in self._SPECIAL_KEYS:
                keys.add(self._SPECIAL_KEYS[part])
            elif len(part) == 1:
                keys.add(keyboard.KeyCode.from_char(part))
            else:
                logger.warning("Unknown hotkey part: %s", part)
        return keys

    def _on_key_press(self, key):
        self._current_keys.add(self._normalize_key(key))

        # Check for Escape (cancel recording)
        if key == keyboard.Key.esc and self._hotkey_pressed:
            self._hotkey_pressed = False
            if self.on_cancel:
                self.on_cancel()
            return

        # Check if all hotkey keys are pressed
        if not self._hotkey_pressed and self._hotkey_keys.issubset(self._current_keys):
            self._hotkey_pressed = True
            if self.on_press:
                self.on_press()

    def _on_key_release(self, key):
        normalized = self._normalize_key(key)
        self._current_keys.discard(normalized)

        # If hotkey was pressed and any key in combo is released, trigger release
        if self._hotkey_pressed and normalized in self._hotkey_keys:
            self._hotkey_pressed = False
            if self.on_release:
                self.on_release()

    def _normalize_key(self, key):
        """Normalize key for consistent comparison."""
        if isinstance(key, keyboard.KeyCode) and key.char:
            return keyboard.KeyCode.from_char(key.char.lower())
        # Map left/right modifiers to generic
        if key in (keyboard.Key.cmd_l, keyboard.Key.cmd_r):
            return keyboard.Key.cmd
        if key in (keyboard.Key.shift_l, keyboard.Key.shift_r):
            return keyboard.Key.shift
        if key in (keyboard.Key.ctrl_l, keyboard.Key.ctrl_r):
            return keyboard.Key.ctrl
        if key in (keyboard.Key.alt_l, keyboard.Key.alt_r):
            return keyboard.Key.alt
        return key

    def start(self) -> None:
        """Start listening for the global hotkey in a background thread."""
        self._listener = keyboard.Listener(
            on_press=self._on_key_press,
            on_release=self._on_key_release,
        )
        self._listener.daemon = True
        self._listener.start()
        logger.info("Hotkey listener started: %s", self.hotkey)

    def stop(self) -> None:
        """Stop the hotkey listener."""
        if self._listener:
            self._listener.stop()
            self._listener = None
            logger.info("Hotkey listener stopped")

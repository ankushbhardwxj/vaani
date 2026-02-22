"""Global hotkey listener using NSEvent (AppKit) — main-thread-safe, no TSM calls.

pynput's keyboard.Listener calls TSMGetInputSourceProperty from a background thread
to convert keycodes to Unicode characters. macOS 15 added a strict dispatch_assert_queue
check that kills the process when this happens. NSEvent.addGlobalMonitorForEventsMatchingMask
delivers events on the main thread and never touches the Text Services Manager.
"""

import logging
from typing import Callable, Optional

try:
    from AppKit import NSEvent as _NSEvent
except ImportError:
    _NSEvent = None  # Non-macOS / test environments

logger = logging.getLogger(__name__)

DEFAULT_HOTKEY = "<alt>"

# NSEvent modifier flag masks (NSEventModifierFlag*)
_MODIFIER_FLAGS = {
    "alt": 1 << 19, "<alt>": 1 << 19,
    "cmd": 1 << 20, "<cmd>": 1 << 20,
    "shift": 1 << 17, "<shift>": 1 << 17,
    "ctrl": 1 << 18, "<ctrl>": 1 << 18,
}

# Special characters for non-modifier key combos
_SPECIAL_CHARS = {
    "space": " ", "<space>": " ",
    "esc": "\x1b", "<esc>": "\x1b",
    "tab": "\t", "<tab>": "\t",
    "enter": "\r", "<enter>": "\r",
}

# NSEvent type constants
_NSKeyDown = 10
_NSKeyUp = 11
_NSFlagsChanged = 12

# NSEvent event mask for addGlobalMonitorForEventsMatchingMask_handler_
_MONITOR_MASK = (1 << 10) | (1 << 11) | (1 << 12)  # KeyDown | KeyUp | FlagsChanged

# Bitmask to extract only the four main modifier keys from NSEvent modifierFlags
_MODIFIER_KEY_MASK = (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20)

# Hardware keycode for Escape (layout-independent)
_ESC_KEYCODE = 53


class HotkeyListener:
    """Detects global hotkey press/release using NSEvent.

    The hotkey works in press-and-hold mode:
    - Press hotkey combo → on_press callback
    - Release hotkey combo → on_release callback
    - Press Escape during recording → on_cancel callback

    start() must be called from the main thread (before or during the NSRunLoop).
    Event callbacks are delivered on the main thread — no TSM thread-safety issues.
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
        self._monitor = None
        self._hotkey_pressed = False
        self._modifier_flags, self._trigger_char = self._parse_hotkey(hotkey)

    @staticmethod
    def _parse_hotkey(hotkey_str: str) -> tuple[int, Optional[str]]:
        """Parse 'alt' or '<cmd>+<shift>+;' into (modifier_mask, trigger_char | None)."""
        modifier_mask = 0
        trigger_char = None
        for part in hotkey_str.lower().split("+"):
            part = part.strip()
            if part in _MODIFIER_FLAGS:
                modifier_mask |= _MODIFIER_FLAGS[part]
            elif part in _SPECIAL_CHARS:
                trigger_char = _SPECIAL_CHARS[part]
            elif len(part) == 1:
                trigger_char = part
            else:
                logger.warning("Unknown hotkey part: %s", part)
        return modifier_mask, trigger_char

    def start(self) -> None:
        """Register the global NSEvent monitor. Must be called from the main thread."""
        self._monitor = _NSEvent.addGlobalMonitorForEventsMatchingMask_handler_(
            _MONITOR_MASK, self._handle_event
        )
        logger.info("Hotkey listener started: %s", self.hotkey)

    def stop(self) -> None:
        """Remove the NSEvent monitor."""
        if self._monitor is not None:
            _NSEvent.removeMonitor_(self._monitor)
            self._monitor = None
            logger.info("Hotkey listener stopped")

    def _handle_event(self, event) -> None:
        """Handle an NSEvent to detect hotkey press, release, and escape-cancel."""
        event_type = event.type()
        modifiers = event.modifierFlags() & _MODIFIER_KEY_MASK

        if self._trigger_char is None:
            # Modifier-only hotkey (e.g. "alt", "cmd")
            if event_type == _NSFlagsChanged:
                required_held = (modifiers & self._modifier_flags) == self._modifier_flags
                if required_held and not self._hotkey_pressed:
                    self._hotkey_pressed = True
                    if self.on_press:
                        self.on_press()
                elif not required_held and self._hotkey_pressed:
                    self._hotkey_pressed = False
                    if self.on_release:
                        self.on_release()
        else:
            # Combo hotkey with a non-modifier character (e.g. "<cmd>+<shift>+;")
            if event_type == _NSKeyDown:
                char = (event.charactersIgnoringModifiers() or "").lower()
                mods_ok = (modifiers & self._modifier_flags) == self._modifier_flags
                if char == self._trigger_char and mods_ok and not self._hotkey_pressed:
                    self._hotkey_pressed = True
                    if self.on_press:
                        self.on_press()
            elif event_type == _NSKeyUp and self._hotkey_pressed:
                char = (event.charactersIgnoringModifiers() or "").lower()
                if char == self._trigger_char:
                    self._hotkey_pressed = False
                    if self.on_release:
                        self.on_release()

        # Escape cancels an in-progress recording regardless of hotkey type
        if event_type == _NSKeyDown and event.keyCode() == _ESC_KEYCODE and self._hotkey_pressed:
            self._hotkey_pressed = False
            if self.on_cancel:
                self.on_cancel()

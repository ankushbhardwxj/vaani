"""Tests for vaani.hotkey — NSEvent-based hotkey parsing and dispatch."""

from unittest.mock import MagicMock, patch

import pytest

from vaani.hotkey import (
    HotkeyListener,
    _ESC_KEYCODE,
    _MODIFIER_FLAGS,
    _NSFlagsChanged,
    _NSKeyDown,
    _NSKeyUp,
)


def _make_event(event_type, modifiers=0, char="", keycode=0):
    """Build a minimal mock NSEvent."""
    event = MagicMock()
    event.type.return_value = event_type
    event.modifierFlags.return_value = modifiers
    event.charactersIgnoringModifiers.return_value = char
    event.keyCode.return_value = keycode
    return event


# ---------------------------------------------------------------------------
# Hotkey parsing
# ---------------------------------------------------------------------------

class TestKeyParsing:
    def test_single_modifier(self):
        hl = HotkeyListener(hotkey="alt")
        assert hl._modifier_flags == _MODIFIER_FLAGS["alt"]
        assert hl._trigger_char is None

    def test_angle_bracket_variant(self):
        hl = HotkeyListener(hotkey="<alt>")
        assert hl._modifier_flags == _MODIFIER_FLAGS["alt"]
        assert hl._trigger_char is None

    def test_cmd_shift_semicolon(self):
        hl = HotkeyListener(hotkey="<cmd>+<shift>+;")
        assert hl._modifier_flags == _MODIFIER_FLAGS["cmd"] | _MODIFIER_FLAGS["shift"]
        assert hl._trigger_char == ";"

    def test_ctrl_a(self):
        hl = HotkeyListener(hotkey="<ctrl>+a")
        assert hl._modifier_flags == _MODIFIER_FLAGS["ctrl"]
        assert hl._trigger_char == "a"

    def test_space_key(self):
        hl = HotkeyListener(hotkey="<cmd>+space")
        assert hl._modifier_flags == _MODIFIER_FLAGS["cmd"]
        assert hl._trigger_char == " "

    def test_multiple_modifiers(self):
        hl = HotkeyListener(hotkey="<cmd>+<shift>+<alt>")
        expected = _MODIFIER_FLAGS["cmd"] | _MODIFIER_FLAGS["shift"] | _MODIFIER_FLAGS["alt"]
        assert hl._modifier_flags == expected
        assert hl._trigger_char is None


# ---------------------------------------------------------------------------
# Callback dispatch via _handle_event
# ---------------------------------------------------------------------------

class TestModifierOnlyHotkey:
    def test_press_fires_on_press(self):
        on_press = MagicMock()
        hl = HotkeyListener(hotkey="alt", on_press=on_press)

        hl._handle_event(_make_event(_NSFlagsChanged, modifiers=_MODIFIER_FLAGS["alt"]))
        on_press.assert_called_once()

    def test_release_fires_on_release(self):
        on_press = MagicMock()
        on_release = MagicMock()
        hl = HotkeyListener(hotkey="alt", on_press=on_press, on_release=on_release)

        hl._handle_event(_make_event(_NSFlagsChanged, modifiers=_MODIFIER_FLAGS["alt"]))
        hl._handle_event(_make_event(_NSFlagsChanged, modifiers=0))

        on_press.assert_called_once()
        on_release.assert_called_once()

    def test_no_double_press(self):
        on_press = MagicMock()
        hl = HotkeyListener(hotkey="alt", on_press=on_press)

        mods = _MODIFIER_FLAGS["alt"]
        hl._handle_event(_make_event(_NSFlagsChanged, modifiers=mods))
        hl._handle_event(_make_event(_NSFlagsChanged, modifiers=mods))

        on_press.assert_called_once()

    def test_no_release_without_press(self):
        on_release = MagicMock()
        hl = HotkeyListener(hotkey="alt", on_release=on_release)

        hl._handle_event(_make_event(_NSFlagsChanged, modifiers=0))
        on_release.assert_not_called()


class TestComboHotkey:
    def test_press_fires_on_press(self):
        on_press = MagicMock()
        hl = HotkeyListener(hotkey="<cmd>+<shift>+;", on_press=on_press)

        mods = _MODIFIER_FLAGS["cmd"] | _MODIFIER_FLAGS["shift"]
        hl._handle_event(_make_event(_NSKeyDown, modifiers=mods, char=";"))
        on_press.assert_called_once()

    def test_key_release_fires_on_release(self):
        on_press = MagicMock()
        on_release = MagicMock()
        hl = HotkeyListener(hotkey="<cmd>+<shift>+;", on_press=on_press, on_release=on_release)

        mods = _MODIFIER_FLAGS["cmd"] | _MODIFIER_FLAGS["shift"]
        hl._handle_event(_make_event(_NSKeyDown, modifiers=mods, char=";"))
        hl._handle_event(_make_event(_NSKeyUp, modifiers=mods, char=";"))

        on_press.assert_called_once()
        on_release.assert_called_once()

    def test_partial_modifiers_do_not_trigger(self):
        on_press = MagicMock()
        hl = HotkeyListener(hotkey="<cmd>+<shift>+;", on_press=on_press)

        # Only cmd, missing shift
        hl._handle_event(_make_event(_NSKeyDown, modifiers=_MODIFIER_FLAGS["cmd"], char=";"))
        on_press.assert_not_called()

    def test_wrong_char_does_not_trigger(self):
        on_press = MagicMock()
        hl = HotkeyListener(hotkey="<cmd>+;", on_press=on_press)

        hl._handle_event(_make_event(_NSKeyDown, modifiers=_MODIFIER_FLAGS["cmd"], char="a"))
        on_press.assert_not_called()


class TestEscapeCancel:
    def test_escape_cancels_modifier_hotkey(self):
        on_press = MagicMock()
        on_cancel = MagicMock()
        hl = HotkeyListener(hotkey="alt", on_press=on_press, on_cancel=on_cancel)

        # Press hotkey
        hl._handle_event(_make_event(_NSFlagsChanged, modifiers=_MODIFIER_FLAGS["alt"]))
        # Escape while pressed
        hl._handle_event(_make_event(_NSKeyDown, keycode=_ESC_KEYCODE))

        on_press.assert_called_once()
        on_cancel.assert_called_once()

    def test_escape_cancels_combo_hotkey(self):
        on_press = MagicMock()
        on_cancel = MagicMock()
        hl = HotkeyListener(hotkey="<cmd>+;", on_press=on_press, on_cancel=on_cancel)

        mods = _MODIFIER_FLAGS["cmd"]
        hl._handle_event(_make_event(_NSKeyDown, modifiers=mods, char=";"))
        hl._handle_event(_make_event(_NSKeyDown, keycode=_ESC_KEYCODE))

        on_cancel.assert_called_once()

    def test_escape_without_active_hotkey_does_not_cancel(self):
        on_cancel = MagicMock()
        hl = HotkeyListener(hotkey="alt", on_cancel=on_cancel)

        hl._handle_event(_make_event(_NSKeyDown, keycode=_ESC_KEYCODE))
        on_cancel.assert_not_called()


# ---------------------------------------------------------------------------
# start / stop
# ---------------------------------------------------------------------------

class TestStartStop:
    def test_start_registers_monitor(self):
        mock_nsevent = MagicMock()
        mock_monitor = MagicMock()
        mock_nsevent.addGlobalMonitorForEventsMatchingMask_handler_.return_value = mock_monitor

        with patch("vaani.hotkey._NSEvent", mock_nsevent):
            hl = HotkeyListener(hotkey="alt")
            hl.start()

        mock_nsevent.addGlobalMonitorForEventsMatchingMask_handler_.assert_called_once()
        assert hl._monitor is mock_monitor

    def test_stop_removes_monitor(self):
        mock_nsevent = MagicMock()
        mock_monitor = MagicMock()

        with patch("vaani.hotkey._NSEvent", mock_nsevent):
            hl = HotkeyListener(hotkey="alt")
            hl._monitor = mock_monitor
            hl.stop()

        mock_nsevent.removeMonitor_.assert_called_once_with(mock_monitor)
        assert hl._monitor is None

    def test_stop_is_idempotent(self):
        mock_nsevent = MagicMock()

        with patch("vaani.hotkey._NSEvent", mock_nsevent):
            hl = HotkeyListener(hotkey="alt")
            hl.stop()  # _monitor is already None
            hl.stop()

        mock_nsevent.removeMonitor_.assert_not_called()

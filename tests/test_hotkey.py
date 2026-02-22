"""Tests for vaani.hotkey — key parsing + callback dispatch (no start(), no macOS)."""

from unittest.mock import MagicMock

import pytest
from pynput import keyboard

from vaani.hotkey import HotkeyListener


# ---------------------------------------------------------------------------
# Key parsing
# ---------------------------------------------------------------------------

class TestKeyParsing:
    def test_cmd_shift_semicolon(self):
        hl = HotkeyListener(hotkey="<cmd>+<shift>+;")
        assert keyboard.Key.cmd in hl._hotkey_keys
        assert keyboard.Key.shift in hl._hotkey_keys
        assert keyboard.KeyCode.from_char(";") in hl._hotkey_keys

    def test_single_modifier(self):
        hl = HotkeyListener(hotkey="alt")
        assert keyboard.Key.alt in hl._hotkey_keys
        assert len(hl._hotkey_keys) == 1

    def test_ctrl_key(self):
        hl = HotkeyListener(hotkey="<ctrl>+a")
        assert keyboard.Key.ctrl in hl._hotkey_keys
        assert keyboard.KeyCode.from_char("a") in hl._hotkey_keys

    def test_space_key(self):
        hl = HotkeyListener(hotkey="<cmd>+space")
        assert keyboard.Key.cmd in hl._hotkey_keys
        assert keyboard.Key.space in hl._hotkey_keys


# ---------------------------------------------------------------------------
# Key normalization
# ---------------------------------------------------------------------------

class TestKeyNormalization:
    def test_cmd_l_to_cmd(self):
        hl = HotkeyListener()
        assert hl._normalize_key(keyboard.Key.cmd_l) == keyboard.Key.cmd

    def test_cmd_r_to_cmd(self):
        hl = HotkeyListener()
        assert hl._normalize_key(keyboard.Key.cmd_r) == keyboard.Key.cmd

    def test_shift_l_to_shift(self):
        hl = HotkeyListener()
        assert hl._normalize_key(keyboard.Key.shift_l) == keyboard.Key.shift

    def test_alt_r_to_alt(self):
        hl = HotkeyListener()
        assert hl._normalize_key(keyboard.Key.alt_r) == keyboard.Key.alt

    def test_char_lowercased(self):
        hl = HotkeyListener()
        key = keyboard.KeyCode.from_char("A")
        normalized = hl._normalize_key(key)
        assert normalized == keyboard.KeyCode.from_char("a")


# ---------------------------------------------------------------------------
# Callback dispatch (direct _on_key_press / _on_key_release)
# ---------------------------------------------------------------------------

class TestCallbackDispatch:
    def test_hotkey_triggers_on_press(self):
        on_press = MagicMock()
        hl = HotkeyListener(hotkey="<cmd>+<shift>+;", on_press=on_press)

        hl._on_key_press(keyboard.Key.cmd_l)
        hl._on_key_press(keyboard.Key.shift_l)
        hl._on_key_press(keyboard.KeyCode.from_char(";"))

        on_press.assert_called_once()

    def test_release_triggers_on_release(self):
        on_press = MagicMock()
        on_release = MagicMock()
        hl = HotkeyListener(
            hotkey="<cmd>+<shift>+;", on_press=on_press, on_release=on_release
        )

        # Press all keys
        hl._on_key_press(keyboard.Key.cmd_l)
        hl._on_key_press(keyboard.Key.shift_l)
        hl._on_key_press(keyboard.KeyCode.from_char(";"))

        # Release one combo key
        hl._on_key_release(keyboard.Key.cmd_l)
        on_release.assert_called_once()

    def test_escape_triggers_cancel(self):
        on_press = MagicMock()
        on_cancel = MagicMock()
        hl = HotkeyListener(
            hotkey="<cmd>+<shift>+;", on_press=on_press, on_cancel=on_cancel
        )

        # Press hotkey first
        hl._on_key_press(keyboard.Key.cmd_l)
        hl._on_key_press(keyboard.Key.shift_l)
        hl._on_key_press(keyboard.KeyCode.from_char(";"))

        # Then press Escape while hotkey is held
        hl._on_key_press(keyboard.Key.esc)
        on_cancel.assert_called_once()

    def test_partial_hotkey_does_not_trigger(self):
        on_press = MagicMock()
        hl = HotkeyListener(hotkey="<cmd>+<shift>+;", on_press=on_press)

        hl._on_key_press(keyboard.Key.cmd_l)
        hl._on_key_press(keyboard.Key.shift_l)
        # Missing semicolon

        on_press.assert_not_called()

    def test_escape_without_hotkey_does_not_cancel(self):
        on_cancel = MagicMock()
        hl = HotkeyListener(hotkey="<cmd>+;", on_cancel=on_cancel)

        hl._on_key_press(keyboard.Key.esc)
        on_cancel.assert_not_called()

    def test_single_modifier_hotkey(self):
        on_press = MagicMock()
        hl = HotkeyListener(hotkey="alt", on_press=on_press)

        hl._on_key_press(keyboard.Key.alt_l)
        on_press.assert_called_once()

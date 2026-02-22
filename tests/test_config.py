"""Tests for vaani.config — VaaniConfig, load/save, API key resolution."""

import os

import pytest
import yaml

from vaani.config import (
    VaaniConfig,
    get_api_key,
    get_or_create_fernet_key,
    load_config,
    save_config,
    set_api_key,
)


# ---------------------------------------------------------------------------
# Default values
# ---------------------------------------------------------------------------

class TestDefaults:
    def test_default_hotkey(self):
        c = VaaniConfig()
        assert c.hotkey == "alt"

    def test_default_sample_rate(self):
        c = VaaniConfig()
        assert c.sample_rate == 16000

    def test_default_active_mode(self):
        c = VaaniConfig()
        assert c.active_mode == "professional"

    def test_default_vad_threshold(self):
        c = VaaniConfig()
        assert c.vad_threshold == 0.05


# ---------------------------------------------------------------------------
# YAML save/load round-trip
# ---------------------------------------------------------------------------

class TestSaveLoad:
    def test_round_trip(self, tmp_vaani_dir):
        original = VaaniConfig(hotkey="<cmd>+k", active_mode="casual")
        save_config(original)
        loaded = load_config()
        assert loaded.hotkey == "<cmd>+k"
        assert loaded.active_mode == "casual"

    def test_invalid_yaml_falls_back_to_defaults(self, tmp_vaani_dir, monkeypatch):
        config_file = tmp_vaani_dir / "config.yaml"
        config_file.write_text(":::invalid yaml{{{")
        loaded = load_config()
        # Should get defaults, not crash
        assert loaded.hotkey == "alt"

    def test_missing_file_returns_defaults(self, tmp_vaani_dir):
        loaded = load_config()
        assert loaded.hotkey == "alt"


# ---------------------------------------------------------------------------
# API key resolution priority
# ---------------------------------------------------------------------------

class TestApiKeyResolution:
    def test_keychain_has_priority(self, mock_keyring, monkeypatch):
        mock_keyring.set_password("vaani", "openai_api_key", "sk-from-keychain")
        monkeypatch.setenv("VAANI_OPENAI_API_KEY", "sk-from-env")
        result = get_api_key("openai_api_key")
        assert result == "sk-from-keychain"

    def test_env_var_fallback(self, mock_keyring, monkeypatch):
        monkeypatch.setenv("VAANI_OPENAI_API_KEY", "sk-from-env")
        result = get_api_key("openai_api_key")
        assert result == "sk-from-env"

    def test_returns_none_when_no_key(self, mock_keyring):
        result = get_api_key("openai_api_key")
        assert result is None

    def test_set_and_get_key(self, mock_keyring):
        set_api_key("openai_api_key", "sk-test-123")
        assert get_api_key("openai_api_key") == "sk-test-123"


# ---------------------------------------------------------------------------
# Fernet key management
# ---------------------------------------------------------------------------

class TestFernetKey:
    def test_creates_key_on_first_call(self, mock_keyring):
        key = get_or_create_fernet_key()
        assert isinstance(key, bytes)
        assert len(key) > 0

    def test_returns_same_key_on_second_call(self, mock_keyring):
        key1 = get_or_create_fernet_key()
        key2 = get_or_create_fernet_key()
        assert key1 == key2

"""Shared fixtures for Vaani tests."""

import os
import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest
from cryptography.fernet import Fernet


# ---------------------------------------------------------------------------
# Mock keyring: dict-backed replacement for macOS Keychain
# ---------------------------------------------------------------------------

class _MockKeyring:
    """In-memory keyring replacement so tests never touch real Keychain."""

    def __init__(self):
        self._store: dict[tuple[str, str], str] = {}

    def get_password(self, service: str, name: str):
        return self._store.get((service, name))

    def set_password(self, service: str, name: str, value: str):
        self._store[(service, name)] = value


@pytest.fixture(autouse=True)
def mock_keyring(monkeypatch):
    """Replace keyring module globally so no test hits macOS Keychain."""
    kr = _MockKeyring()
    monkeypatch.setattr("keyring.get_password", kr.get_password)
    monkeypatch.setattr("keyring.set_password", kr.set_password)
    # Also block bundled keys file so tests never pick up real API keys
    monkeypatch.setattr("vaani.config._get_bundled_keys", lambda: {})
    return kr


# ---------------------------------------------------------------------------
# Fernet key fixture
# ---------------------------------------------------------------------------

@pytest.fixture
def fernet_key():
    """Generate a fresh Fernet key for storage tests."""
    return Fernet.generate_key()


# ---------------------------------------------------------------------------
# Temporary Vaani directory
# ---------------------------------------------------------------------------

@pytest.fixture
def tmp_vaani_dir(tmp_path, monkeypatch):
    """Redirect VAANI_DIR, CONFIG_FILE, and prompt paths to a temp dir."""
    vaani_dir = tmp_path / ".vaani"
    vaani_dir.mkdir()
    (vaani_dir / "prompts" / "modes").mkdir(parents=True)
    (vaani_dir / "pending").mkdir()

    monkeypatch.setattr("vaani.config.VAANI_DIR", vaani_dir)
    monkeypatch.setattr("vaani.config.CONFIG_FILE", vaani_dir / "config.yaml")

    # Also patch the storage module's DB_PATH if it has been imported
    try:
        import vaani.storage
        monkeypatch.setattr("vaani.storage.DB_PATH", vaani_dir / "history.db")
    except ImportError:
        pass

    # Patch enhance prompt paths
    try:
        import vaani.enhance
        monkeypatch.setattr("vaani.enhance._USER_PROMPTS", vaani_dir / "prompts")
    except ImportError:
        pass

    return vaani_dir

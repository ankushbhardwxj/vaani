"""Configuration management with pydantic-settings, YAML, and macOS Keychain."""

import logging
import os
from pathlib import Path
from typing import Optional

import keyring
import yaml
from pydantic import Field
from pydantic_settings import BaseSettings

logger = logging.getLogger(__name__)

VAANI_DIR = Path.home() / ".vaani"
CONFIG_FILE = VAANI_DIR / "config.yaml"
KEYRING_SERVICE = "vaani"


class VaaniConfig(BaseSettings):
    """Vaani configuration, loaded from ~/.vaani/config.yaml."""

    # Hotkey
    hotkey: str = Field(default="alt", description="Global hotkey combo")

    # Audio
    sample_rate: int = 16000
    vad_threshold: float = 0.05
    max_recording_seconds: int = 600  # 10 minutes

    # STT
    stt_model: str = "whisper-1"

    # LLM
    llm_model: str = "claude-haiku-4-5-20251001"
    active_mode: str = "professional"

    # Audio feedback
    sounds_enabled: bool = True

    # Paste timing
    paste_restore_delay_ms: int = 100

    # Logging
    log_file: str = str(VAANI_DIR / "vaani.log")
    log_max_bytes: int = 5 * 1024 * 1024  # 5MB
    log_backup_count: int = 3

    # Launch at login
    launch_at_login: bool = False

    model_config = {"env_prefix": "VAANI_"}


def ensure_vaani_dir() -> None:
    """Create ~/.vaani and subdirectories if they don't exist."""
    for subdir in ["", "prompts/modes", "pending"]:
        (VAANI_DIR / subdir).mkdir(parents=True, exist_ok=True)


def load_config() -> VaaniConfig:
    """Load config from YAML file, falling back to defaults."""
    ensure_vaani_dir()
    if CONFIG_FILE.exists():
        try:
            data = yaml.safe_load(CONFIG_FILE.read_text()) or {}
            return VaaniConfig(**data)
        except Exception:
            logger.exception("Failed to load config, using defaults")
    return VaaniConfig()


def save_config(config: VaaniConfig) -> None:
    """Save config to YAML file."""
    ensure_vaani_dir()
    data = config.model_dump()
    CONFIG_FILE.write_text(yaml.dump(data, default_flow_style=False, sort_keys=False))


# --- API Key Management via Keyring ---


def set_api_key(name: str, key: str) -> None:
    """Store an API key in macOS Keychain."""
    keyring.set_password(KEYRING_SERVICE, name, key)
    logger.info("Stored %s key in Keychain", name)


def _get_bundled_keys() -> dict:
    """Load API keys from bundled keys file (for standalone .app distribution)."""
    # Check next to the running script/frozen app
    bundled_paths = [
        Path(__file__).parent.parent.parent / "api_keys.yaml",  # dev: project root
        Path(os.path.dirname(os.path.abspath(__file__))) / "api_keys.yaml",
    ]
    # PyInstaller sets sys._MEIPASS for bundled apps
    import sys
    if getattr(sys, "_MEIPASS", None):
        bundled_paths.insert(0, Path(sys._MEIPASS) / "api_keys.yaml")

    for path in bundled_paths:
        if path.exists():
            try:
                data = yaml.safe_load(path.read_text()) or {}
                return data
            except Exception:
                logger.debug("Failed to load bundled keys from %s", path)
    return {}


def get_api_key(name: str) -> Optional[str]:
    """Retrieve an API key: Keychain → env var → bundled keys file."""
    # 1. Try Keychain
    try:
        val = keyring.get_password(KEYRING_SERVICE, name)
        if val:
            return val
    except Exception:
        pass

    # 2. Try environment variable (e.g. VAANI_OPENAI_API_KEY)
    env_val = os.environ.get(f"VAANI_{name.upper()}")
    if env_val:
        return env_val

    # 3. Try bundled keys file
    bundled = _get_bundled_keys()
    return bundled.get(name)


def get_openai_key() -> Optional[str]:
    return get_api_key("openai_api_key")


def get_anthropic_key() -> Optional[str]:
    return get_api_key("anthropic_api_key")


# --- Encryption Key for Storage ---


def get_or_create_fernet_key() -> bytes:
    """Get or auto-generate a Fernet key, stored in Keychain."""
    existing = get_api_key("fernet_key")
    if existing:
        return existing.encode()

    from cryptography.fernet import Fernet

    key = Fernet.generate_key()
    set_api_key("fernet_key", key.decode())
    return key

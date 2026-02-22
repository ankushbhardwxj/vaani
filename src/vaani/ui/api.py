"""Python backend exposed to the pywebview JS frontend."""

import logging
import threading
from typing import Optional

logger = logging.getLogger(__name__)


def _config():
    """Lazy import of vaani.config to avoid pulling in pydantic/keyring/yaml at module load."""
    import vaani.config as _cfg
    return _cfg


class VaaniAPI:
    """Python↔JS bridge for the settings panel and onboarding wizard."""

    def __init__(self) -> None:
        self._recorder = None
        self._mic_test_thread: Optional[threading.Thread] = None
        self._mic_testing = threading.Event()
        self._current_level: float = 0.0
        self._window = None  # set by launcher to allow close_window()

    # --- Window ---

    def close_window(self) -> dict:
        """Close the window (pywebview .destroy() or native NSWindow .close())."""
        try:
            if self._window:
                if hasattr(self._window, "destroy"):
                    self._window.destroy()   # pywebview (onboarding / CLI)
                else:
                    self._window.close()     # native NSWindow (in-process)
            return {"ok": True}
        except Exception as e:
            logger.exception("Failed to close window")
            return {"error": str(e)}

    # --- Config ---

    def get_config(self) -> dict:
        """Return the current config as a dict."""
        try:
            cfg = _config()
            config = cfg.load_config()
            return config.model_dump()
        except Exception as e:
            logger.exception("Failed to load config")
            return {"error": str(e)}

    def save_config(self, data: dict) -> dict:
        """Save config fields. Merges with existing config."""
        try:
            cfg = _config()
            config = cfg.load_config()
            current = config.model_dump()
            current.update(data)
            new_config = cfg.VaaniConfig(**current)
            cfg.save_config(new_config)
            return {"ok": True}
        except Exception as e:
            logger.exception("Failed to save config")
            return {"error": str(e)}

    # --- API Keys ---

    def get_api_keys_status(self) -> dict:
        """Return which API keys are configured."""
        cfg = _config()
        return {
            "openai": bool(cfg.get_openai_key()),
            "anthropic": bool(cfg.get_anthropic_key()),
        }

    def set_api_key(self, provider: str, key: str) -> dict:
        """Store an API key in the Keychain."""
        try:
            key = key.strip()
            if not key:
                return {"error": "Key cannot be empty"}
            key_name = f"{provider}_api_key"
            _config().set_api_key(key_name, key)
            return {"ok": True}
        except Exception as e:
            logger.exception("Failed to set API key")
            return {"error": str(e)}

    # --- Microphone ---

    def list_microphones(self) -> list:
        """Return available input microphones."""
        try:
            from vaani.audio import list_microphones
            return list_microphones()
        except Exception as e:
            logger.exception("Failed to list microphones")
            return []

    def start_mic_test(self, device_index=None) -> dict:
        """Start recording from a mic to show real-time levels."""
        try:
            self.stop_mic_test()

            from vaani.audio import AudioRecorder
            self._recorder = AudioRecorder(sample_rate=16000, device=device_index)
            self._recorder.start()
            self._mic_testing.set()

            def _poll_level():
                while self._mic_testing.is_set():
                    if self._recorder:
                        self._current_level = self._recorder.current_level
                    threading.Event().wait(0.05)

            self._mic_test_thread = threading.Thread(target=_poll_level, daemon=True)
            self._mic_test_thread.start()
            return {"ok": True}
        except Exception as e:
            logger.exception("Failed to start mic test")
            return {"error": str(e)}

    def get_mic_level(self) -> float:
        """Return current RMS level (0.0–1.0) during mic test."""
        return self._current_level

    def stop_mic_test(self) -> dict:
        """Stop the mic test."""
        try:
            self._mic_testing.clear()
            if self._recorder:
                self._recorder.cancel()
                self._recorder = None
            self._current_level = 0.0
            return {"ok": True}
        except Exception as e:
            logger.exception("Failed to stop mic test")
            return {"error": str(e)}

    # --- Hotkey ---

    def get_hotkey(self) -> str:
        """Return the current hotkey string."""
        config = _config().load_config()
        return config.hotkey

    def set_hotkey(self, hotkey: str) -> dict:
        """Validate and save a new hotkey."""
        try:
            hotkey = hotkey.strip()
            if not hotkey:
                return {"error": "Hotkey cannot be empty"}
            return self.save_config({"hotkey": hotkey})
        except Exception as e:
            return {"error": str(e)}

    # --- Permissions (onboarding) ---

    def check_permissions(self) -> dict:
        """Check macOS permissions (best-effort)."""
        perms = {"microphone": False, "accessibility": False}
        try:
            import sounddevice as sd
            sd.query_devices()
            perms["microphone"] = True
        except Exception:
            pass
        try:
            import subprocess
            result = subprocess.run(
                ["osascript", "-e", 'tell application "System Events" to return name of first process'],
                capture_output=True, timeout=5,
            )
            perms["accessibility"] = result.returncode == 0
        except Exception:
            pass
        return perms

    def complete_onboarding(self) -> dict:
        """Mark onboarding as completed and save."""
        try:
            cfg = _config()
            config = cfg.load_config()
            config_data = config.model_dump()
            config_data["onboarding_completed"] = True
            new_config = cfg.VaaniConfig(**config_data)
            cfg.save_config(new_config)
            return {"ok": True}
        except Exception as e:
            logger.exception("Failed to complete onboarding")
            return {"error": str(e)}

    # --- Utility ---

    def get_version(self) -> str:
        """Return the app version."""
        return "0.1.0"

    def open_log_file(self) -> dict:
        """Open the log file in the default editor."""
        try:
            import subprocess
            cfg = _config()
            log_path = cfg.VAANI_DIR / "vaani.log"
            if log_path.exists():
                subprocess.Popen(["open", str(log_path)])
            return {"ok": True}
        except Exception as e:
            return {"error": str(e)}

    def open_config_dir(self) -> dict:
        """Open the config directory in Finder."""
        try:
            import subprocess
            cfg = _config()
            subprocess.Popen(["open", str(cfg.VAANI_DIR)])
            return {"ok": True}
        except Exception as e:
            return {"error": str(e)}

"""macOS menu bar app using rumps."""

import logging
import subprocess
import threading
from functools import wraps
from pathlib import Path
from typing import Callable, Optional

import rumps

from vaani.state import AppState


def _on_main_thread(func):
    """Decorator that dispatches the call to the main thread if needed."""
    @wraps(func)
    def wrapper(*args, **kwargs):
        if threading.current_thread() is threading.main_thread():
            return func(*args, **kwargs)
        from PyObjCTools import AppHelper
        AppHelper.callAfter(func, *args, **kwargs)
    return wrapper

logger = logging.getLogger(__name__)

ICON_PATH = Path(__file__).parent.parent.parent / "assets" / "mic_template.png"
SOUNDS_DIR = Path(__file__).parent / "sounds"


class VaaniMenuBar(rumps.App):
    """Native macOS menu bar app for Vaani."""

    def __init__(
        self,
        on_toggle_recording: Optional[Callable] = None,
    ) -> None:
        icon = str(ICON_PATH) if ICON_PATH.exists() else None
        super().__init__("Vaani", icon=icon, template=True, quit_button=None)

        self._on_toggle_recording = on_toggle_recording
        self._level_timer: Optional[rumps.Timer] = None
        self._anim_frame = 0

        self._build_menu()

    def _build_menu(self) -> None:
        self.menu.clear()

        self._record_item = rumps.MenuItem(
            "Start Recording", callback=self._toggle_recording
        )
        self.menu.add(self._record_item)

        self.menu.add(rumps.separator)

        self.menu.add(rumps.MenuItem("Preferences", callback=self._open_preferences))

        self.menu.add(rumps.separator)

        self.menu.add(rumps.MenuItem("Quit Vaani", callback=self._quit))

    def _toggle_recording(self, sender) -> None:
        if self._on_toggle_recording:
            threading.Thread(
                target=self._on_toggle_recording, daemon=True
            ).start()

    def _open_preferences(self, sender) -> None:
        """Open settings window in-process (native NSWindow, instant)."""
        from vaani.ui.settings import open_settings
        open_settings()

    def _quit(self, sender) -> None:
        rumps.quit_application()

    # --- Recording animation ---

    def _start_level_animation(self) -> None:
        self._anim_frame = 0

        def tick(sender):
            dot = "●" if self._anim_frame % 2 == 0 else "○"
            self.title = f"REC {dot}"
            self._anim_frame += 1

        self._level_timer = rumps.Timer(tick, 0.6)
        self._level_timer.start()

    def _stop_level_animation(self) -> None:
        if self._level_timer:
            self._level_timer.stop()
            self._level_timer = None

    # --- State updates ---

    @_on_main_thread
    def update_state(self, state: AppState) -> None:
        """Update menu bar title and recording button based on app state."""
        if state == AppState.IDLE:
            self._stop_level_animation()
            self.title = "Vaani"
            self._record_item.title = "Start Recording"
        elif state == AppState.RECORDING:
            self._record_item.title = "Stop Recording"
            self._start_level_animation()
        elif state == AppState.PROCESSING:
            self._stop_level_animation()
            self.title = "Processing..."
            self._record_item.title = "Processing..."

    @_on_main_thread
    def show_notification(self, title: str, message: str) -> None:
        """Show a macOS notification."""
        rumps.notification(title, "", message)

    def play_sound(self, sound_name: str) -> None:
        """Play a sound by name.

        Looks for a bundled WAV file in the sounds directory first
        (e.g. ``sounds/record_start.wav``), then falls back to a macOS
        system sound (e.g. ``/System/Library/Sounds/<name>.aiff``).
        """
        bundled = SOUNDS_DIR / f"{sound_name}.wav"
        if bundled.exists():
            path = str(bundled)
        else:
            path = f"/System/Library/Sounds/{sound_name}.aiff"

        try:
            subprocess.Popen(
                ["afplay", path],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except Exception:
            logger.debug("Failed to play sound: %s", sound_name)

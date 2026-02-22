"""macOS menu bar app using rumps."""

import logging
import os
import subprocess
import threading
from pathlib import Path
from typing import Callable, Optional

import rumps

from vaani.state import AppState

logger = logging.getLogger(__name__)

MODES = ["cleanup", "professional", "casual", "bullets"]
ICON_PATH = Path(__file__).parent.parent.parent / "assets" / "mic_template.png"


class VaaniMenuBar(rumps.App):
    """Native macOS menu bar app for Vaani."""

    def __init__(
        self,
        on_toggle_recording: Optional[Callable] = None,
        on_mode_change: Optional[Callable[[str], None]] = None,
        active_mode: str = "cleanup",
    ) -> None:
        icon = str(ICON_PATH) if ICON_PATH.exists() else None
        super().__init__("Vaani", icon=icon, template=True, quit_button=None)

        self._on_toggle_recording = on_toggle_recording
        self._on_mode_change = on_mode_change
        self._active_mode = active_mode

        self._build_menu()

    def _build_menu(self) -> None:
        self.menu.clear()

        # Start/Stop recording
        self._record_item = rumps.MenuItem(
            "Start Recording", callback=self._toggle_recording
        )
        self.menu.add(self._record_item)

        self.menu.add(rumps.separator)

        # Mode submenu
        mode_menu = rumps.MenuItem("Mode")
        for mode in MODES:
            item = rumps.MenuItem(
                mode.capitalize(),
                callback=self._make_mode_callback(mode),
            )
            item.state = 1 if mode == self._active_mode else 0
            mode_menu.add(item)
        self.menu.add(mode_menu)

        self.menu.add(rumps.separator)

        # Quit
        self.menu.add(rumps.MenuItem("Quit Vaani", callback=self._quit))

    def _toggle_recording(self, sender) -> None:
        if self._on_toggle_recording:
            threading.Thread(
                target=self._on_toggle_recording, daemon=True
            ).start()

    def _make_mode_callback(self, mode: str):
        def callback(sender):
            self._active_mode = mode
            # Update checkmarks
            mode_menu = self.menu["Mode"]
            for m in MODES:
                mode_menu[m.capitalize()].state = 1 if m == mode else 0
            if self._on_mode_change:
                self._on_mode_change(mode)
            logger.info("Mode changed to: %s", mode)
        return callback

    def _quit(self, sender) -> None:
        rumps.quit_application()

    def update_state(self, state: AppState) -> None:
        """Update menu bar title and recording button based on app state."""
        if state == AppState.IDLE:
            self.title = "Vaani"
            self._record_item.title = "Start Recording"
        elif state == AppState.RECORDING:
            self.title = "Listening..."
            self._record_item.title = "Stop Recording"
        elif state == AppState.PROCESSING:
            self.title = "Processing..."
            self._record_item.title = "Processing..."

    def show_notification(self, title: str, message: str) -> None:
        """Show a macOS notification."""
        rumps.notification(title, "", message)

    def play_sound(self, sound_name: str) -> None:
        """Play a system sound (e.g., 'Tink', 'Pop')."""
        try:
            subprocess.Popen(
                ["afplay", f"/System/Library/Sounds/{sound_name}.aiff"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except Exception:
            logger.debug("Failed to play sound: %s", sound_name)

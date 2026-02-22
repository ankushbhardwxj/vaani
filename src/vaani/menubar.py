"""macOS menu bar app using rumps."""

import logging
import math
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
        get_level: Optional[Callable[[], float]] = None,
        on_microphone_change: Optional[Callable[[Optional[int]], None]] = None,
    ) -> None:
        icon = str(ICON_PATH) if ICON_PATH.exists() else None
        super().__init__("Vaani", icon=icon, template=True, quit_button=None)

        self._on_toggle_recording = on_toggle_recording
        self._on_mode_change = on_mode_change
        self._active_mode = active_mode
        self._get_level = get_level
        self._on_microphone_change = on_microphone_change
        self._active_microphone_index: Optional[int] = None  # Track current selection
        self._level_timer: Optional[rumps.Timer] = None
        self._anim_frame = 0

        self._build_menu()

    def _build_menu(self) -> None:
        self.menu.clear()

        # Start/Stop recording
        self._record_item = rumps.MenuItem(
            "Start Recording", callback=self._toggle_recording
        )
        self.menu.add(self._record_item)

        self.menu.add(rumps.separator)

        # Microphone submenu
        self._microphone_menu = rumps.MenuItem("Microphone")
        self._populate_microphone_menu()
        self.menu.add(self._microphone_menu)

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

    def _populate_microphone_menu(self) -> None:
        """Populate microphone submenu with available devices."""
        try:
            mics = self._list_microphones()

            if not mics:
                self._microphone_menu.add(rumps.MenuItem("No microphones found"))
                return

            # Add "Default" option
            default_item = rumps.MenuItem(
                "Default",
                callback=self._make_microphone_callback(None),
            )
            default_item.state = 1 if self._active_microphone_index is None else 0
            self._microphone_menu.add(default_item)

            # Add each microphone
            for mic in mics:
                item = rumps.MenuItem(
                    mic['name'],
                    callback=self._make_microphone_callback(mic['index']),
                )
                item.state = 1 if mic['index'] == self._active_microphone_index else 0
                self._microphone_menu.add(item)
        except Exception as e:
            logger.exception("Failed to populate microphone menu")
            self._microphone_menu.add(rumps.MenuItem(f"Error: {str(e)[:50]}"))

    def _list_microphones(self) -> list[dict]:
        """Return a list of available input microphones."""
        import sounddevice as sd

        devices = sd.query_devices()
        mics = []
        for i, device in enumerate(devices):
            if device['max_input_channels'] > 0:
                mics.append({
                    'index': i,
                    'name': device['name'],
                })
        return mics

    def _make_microphone_callback(self, device_index: Optional[int]):
        def callback(sender):
            # Update active microphone and refresh menu states
            self._active_microphone_index = device_index
            self._update_microphone_menu_states()
            if self._on_microphone_change:
                self._on_microphone_change(device_index)
        return callback

    def _update_microphone_menu_states(self) -> None:
        """Update checkmark states for all microphone menu items."""
        for item in self._microphone_menu.values():
            # Check if this is the active device
            if item.title == "Default":
                item.state = 1 if self._active_microphone_index is None else 0
            else:
                # Try to find the device index for this item
                mics = self._list_microphones()
                for mic in mics:
                    if mic['name'] == item.title:
                        item.state = 1 if mic['index'] == self._active_microphone_index else 0
                        break

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

    def _start_level_animation(self) -> None:
        """Animate menubar title with pulsing REC dot."""
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

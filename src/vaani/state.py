"""State machine for Vaani app lifecycle."""

import enum
import logging
import threading

logger = logging.getLogger(__name__)


class AppState(enum.Enum):
    IDLE = "idle"
    RECORDING = "recording"
    PROCESSING = "processing"


class StateMachine:
    """Thread-safe state machine: IDLE → RECORDING → PROCESSING → IDLE."""

    _VALID_TRANSITIONS = {
        AppState.IDLE: {AppState.RECORDING},
        AppState.RECORDING: {AppState.PROCESSING, AppState.IDLE},  # IDLE = cancel
        AppState.PROCESSING: {AppState.IDLE},
    }

    def __init__(self) -> None:
        self._state = AppState.IDLE
        self._lock = threading.Lock()
        self._listeners: list = []

    @property
    def state(self) -> AppState:
        return self._state

    def transition(self, target: AppState) -> bool:
        """Attempt state transition. Returns True if successful."""
        with self._lock:
            if target not in self._VALID_TRANSITIONS.get(self._state, set()):
                logger.warning(
                    "Invalid transition %s → %s", self._state.value, target.value
                )
                return False
            old = self._state
            self._state = target
            logger.info("State: %s → %s", old.value, target.value)

        # Notify listeners outside lock
        for cb in self._listeners:
            try:
                cb(old, target)
            except Exception:
                logger.exception("State listener error")
        return True

    def on_change(self, callback) -> None:
        """Register a callback(old_state, new_state)."""
        self._listeners.append(callback)

    @property
    def is_idle(self) -> bool:
        return self._state == AppState.IDLE

    @property
    def is_recording(self) -> bool:
        return self._state == AppState.RECORDING

    @property
    def is_processing(self) -> bool:
        return self._state == AppState.PROCESSING

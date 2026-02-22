"""Tests for vaani.state — pure-logic StateMachine, no mocking needed."""

import threading

import pytest

from vaani.state import AppState, StateMachine


# ---------------------------------------------------------------------------
# Valid transitions
# ---------------------------------------------------------------------------

class TestValidTransitions:
    def test_idle_to_recording(self):
        sm = StateMachine()
        assert sm.transition(AppState.RECORDING) is True
        assert sm.state == AppState.RECORDING

    def test_recording_to_processing(self):
        sm = StateMachine()
        sm.transition(AppState.RECORDING)
        assert sm.transition(AppState.PROCESSING) is True
        assert sm.state == AppState.PROCESSING

    def test_processing_to_idle(self):
        sm = StateMachine()
        sm.transition(AppState.RECORDING)
        sm.transition(AppState.PROCESSING)
        assert sm.transition(AppState.IDLE) is True
        assert sm.state == AppState.IDLE

    def test_recording_to_idle_cancel(self):
        sm = StateMachine()
        sm.transition(AppState.RECORDING)
        assert sm.transition(AppState.IDLE) is True
        assert sm.state == AppState.IDLE


# ---------------------------------------------------------------------------
# Invalid transitions
# ---------------------------------------------------------------------------

class TestInvalidTransitions:
    def test_idle_to_processing(self):
        sm = StateMachine()
        assert sm.transition(AppState.PROCESSING) is False
        assert sm.state == AppState.IDLE

    def test_processing_to_recording(self):
        sm = StateMachine()
        sm.transition(AppState.RECORDING)
        sm.transition(AppState.PROCESSING)
        assert sm.transition(AppState.RECORDING) is False
        assert sm.state == AppState.PROCESSING

    def test_idle_to_idle(self):
        sm = StateMachine()
        assert sm.transition(AppState.IDLE) is False
        assert sm.state == AppState.IDLE


# ---------------------------------------------------------------------------
# Convenience properties
# ---------------------------------------------------------------------------

class TestProperties:
    def test_is_idle_initial(self):
        sm = StateMachine()
        assert sm.is_idle is True
        assert sm.is_recording is False
        assert sm.is_processing is False

    def test_is_recording(self):
        sm = StateMachine()
        sm.transition(AppState.RECORDING)
        assert sm.is_idle is False
        assert sm.is_recording is True
        assert sm.is_processing is False

    def test_is_processing(self):
        sm = StateMachine()
        sm.transition(AppState.RECORDING)
        sm.transition(AppState.PROCESSING)
        assert sm.is_idle is False
        assert sm.is_recording is False
        assert sm.is_processing is True


# ---------------------------------------------------------------------------
# Listener callbacks
# ---------------------------------------------------------------------------

class TestListeners:
    def test_callback_called_on_valid_transition(self):
        sm = StateMachine()
        calls = []
        sm.on_change(lambda old, new: calls.append((old, new)))
        sm.transition(AppState.RECORDING)
        assert calls == [(AppState.IDLE, AppState.RECORDING)]

    def test_callback_not_called_on_invalid_transition(self):
        sm = StateMachine()
        calls = []
        sm.on_change(lambda old, new: calls.append((old, new)))
        sm.transition(AppState.PROCESSING)  # invalid from IDLE
        assert calls == []

    def test_exception_in_callback_does_not_break_state(self):
        sm = StateMachine()

        def bad_callback(old, new):
            raise ValueError("boom")

        sm.on_change(bad_callback)
        assert sm.transition(AppState.RECORDING) is True
        assert sm.state == AppState.RECORDING

    def test_multiple_listeners(self):
        sm = StateMachine()
        a, b = [], []
        sm.on_change(lambda old, new: a.append(new))
        sm.on_change(lambda old, new: b.append(new))
        sm.transition(AppState.RECORDING)
        assert a == [AppState.RECORDING]
        assert b == [AppState.RECORDING]


# ---------------------------------------------------------------------------
# Full lifecycle
# ---------------------------------------------------------------------------

class TestLifecycle:
    def test_full_cycle(self):
        sm = StateMachine()
        assert sm.transition(AppState.RECORDING) is True
        assert sm.transition(AppState.PROCESSING) is True
        assert sm.transition(AppState.IDLE) is True
        assert sm.is_idle is True

    def test_double_cycle(self):
        sm = StateMachine()
        for _ in range(2):
            sm.transition(AppState.RECORDING)
            sm.transition(AppState.PROCESSING)
            sm.transition(AppState.IDLE)
        assert sm.is_idle is True


# ---------------------------------------------------------------------------
# Thread safety
# ---------------------------------------------------------------------------

class TestThreadSafety:
    def test_concurrent_transitions(self):
        sm = StateMachine()
        results = []
        barrier = threading.Barrier(10)

        def try_transition():
            barrier.wait()
            ok = sm.transition(AppState.RECORDING)
            results.append(ok)

        threads = [threading.Thread(target=try_transition) for _ in range(10)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        # Exactly one thread should succeed
        assert results.count(True) == 1
        assert results.count(False) == 9
        assert sm.state == AppState.RECORDING

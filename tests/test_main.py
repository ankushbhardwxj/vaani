"""Tests for vaani.main — VaaniApp orchestrator + CLI commands."""

import threading
from unittest.mock import MagicMock, patch, PropertyMock

import numpy as np
import pytest
from click.testing import CliRunner

from vaani.config import VaaniConfig
from vaani.main import VaaniApp, cli
from vaani.state import AppState


# ---------------------------------------------------------------------------
# VaaniApp state transitions
# ---------------------------------------------------------------------------

class TestVaaniAppStates:
    @pytest.fixture
    def app(self):
        config = VaaniConfig()
        app = VaaniApp(config)
        app._prewarm_done.set()
        mock_recorder = MagicMock()
        mock_recorder.current_level = 0.5
        mock_recorder.stop.return_value = np.zeros(16000, dtype=np.float32)
        app._recorder = mock_recorder
        return app

    def test_start_recording(self, app):
        app.start_recording()
        assert app.state.is_recording

    def test_cancel_recording(self, app):
        app.start_recording()
        app.cancel_recording()
        assert app.state.is_idle

    def test_toggle_starts_from_idle(self, app):
        app.toggle_recording()
        assert app.state.is_recording

    def test_toggle_stops_from_recording(self, app):
        app.start_recording()
        app.toggle_recording()
        # After toggle from recording, should transition to PROCESSING
        assert app.state.is_processing or app.state.is_idle

    def test_start_recording_while_processing_does_nothing(self, app):
        app.start_recording()
        app.state.transition(AppState.PROCESSING)
        app.start_recording()
        # Should still be processing
        assert app.state.is_processing


# ---------------------------------------------------------------------------
# Pipeline (_process_audio) — patches target source modules (local imports)
# ---------------------------------------------------------------------------

class TestProcessAudio:
    @pytest.fixture
    def app(self, tmp_vaani_dir):
        config = VaaniConfig()
        return VaaniApp(config)

    @patch("vaani.output.paste_text")
    @patch("vaani.enhance.enhance")
    @patch("vaani.transcribe.transcribe")
    @patch("vaani.audio.process_audio")
    @patch("vaani.audio.encode_wav")
    def test_full_pipeline_success(
        self, mock_encode, mock_process, mock_transcribe, mock_enhance, mock_paste, app
    ):
        mock_encode.return_value = b"wav"
        mock_process.return_value = b"processed wav"
        mock_transcribe.return_value = "hello world"
        mock_enhance.return_value = "Hello, world!"

        app.state.transition(AppState.RECORDING)
        app.state.transition(AppState.PROCESSING)

        audio = np.random.randn(16000).astype(np.float32)
        app._process_audio(audio)

        mock_transcribe.assert_called_once()
        mock_enhance.assert_called_once()
        mock_paste.assert_called_once_with("Hello, world!", app.config.paste_restore_delay_ms)
        assert app.state.is_idle

    @patch("vaani.audio.encode_wav")
    @patch("vaani.audio.process_audio")
    def test_no_speech_returns_to_idle(self, mock_process, mock_encode, app):
        mock_encode.return_value = b"wav"
        mock_process.return_value = None  # no speech

        app.state.transition(AppState.RECORDING)
        app.state.transition(AppState.PROCESSING)
        app._process_audio(np.zeros(16000, dtype=np.float32))

        assert app.state.is_idle

    @patch("vaani.audio.encode_wav")
    @patch("vaani.audio.process_audio")
    @patch("vaani.transcribe.transcribe")
    def test_junk_transcription_returns_to_idle(
        self, mock_transcribe, mock_process, mock_encode, app
    ):
        mock_encode.return_value = b"wav"
        mock_process.return_value = b"wav bytes"
        mock_transcribe.return_value = "ok"  # too short (<3 chars stripped)

        app.state.transition(AppState.RECORDING)
        app.state.transition(AppState.PROCESSING)
        app._process_audio(np.zeros(16000, dtype=np.float32))

        assert app.state.is_idle

    @patch("vaani.audio.encode_wav")
    @patch("vaani.audio.process_audio")
    @patch("vaani.transcribe.transcribe")
    def test_pipeline_error_returns_to_idle(
        self, mock_transcribe, mock_process, mock_encode, app
    ):
        mock_encode.return_value = b"wav"
        mock_process.return_value = b"wav bytes"
        mock_transcribe.side_effect = RuntimeError("API error")

        app.state.transition(AppState.RECORDING)
        app.state.transition(AppState.PROCESSING)
        app._process_audio(np.zeros(16000, dtype=np.float32))

        assert app.state.is_idle


# ---------------------------------------------------------------------------
# Integration test: full wiring with all externals mocked
# ---------------------------------------------------------------------------

@pytest.mark.integration
class TestIntegration:
    @patch("vaani.output.paste_text")
    @patch("vaani.enhance.enhance")
    @patch("vaani.transcribe.transcribe")
    @patch("vaani.audio.process_audio")
    @patch("vaani.audio.encode_wav")
    def test_end_to_end_pipeline(
        self, mock_encode, mock_process, mock_transcribe, mock_enhance, mock_paste, tmp_vaani_dir
    ):
        """Synthetic audio -> mocked VAD -> mocked Whisper -> mocked Claude -> mocked paste."""
        config = VaaniConfig()
        app = VaaniApp(config)

        # Simulate a sine-wave audio
        t = np.linspace(0, 1, 16000, dtype=np.float32)
        audio = (np.sin(2 * np.pi * 440 * t) * 0.5).astype(np.float32)

        mock_encode.return_value = b"wav"
        mock_process.return_value = b"processed wav bytes"
        mock_transcribe.return_value = "this is a test recording"
        mock_enhance.return_value = "This is a test recording."

        app.state.transition(AppState.RECORDING)
        app.state.transition(AppState.PROCESSING)
        app._process_audio(audio)

        # Verify pipeline executed in order
        mock_process.assert_called_once()
        mock_transcribe.assert_called_once()
        mock_enhance.assert_called_once_with(
            "this is a test recording",
            mode=config.active_mode,
            model=config.llm_model,
        )
        mock_paste.assert_called_once_with(
            "This is a test recording.",
            config.paste_restore_delay_ms,
        )
        assert app.state.is_idle


# ---------------------------------------------------------------------------
# CLI commands
# ---------------------------------------------------------------------------

class TestCLI:
    def test_cli_help(self):
        runner = CliRunner()
        result = runner.invoke(cli, ["--help"])
        assert result.exit_code == 0
        assert "Vaani" in result.output

    def test_setup_command_prompts_for_keys(self, tmp_vaani_dir, mock_keyring):
        runner = CliRunner()
        result = runner.invoke(cli, ["setup"], input="sk-openai-test\nsk-anthropic-test\n")
        assert result.exit_code == 0
        assert "Setup complete" in result.output

    @patch("vaani.main.VaaniApp")
    @patch("vaani.main.load_config")
    @patch("vaani.main.get_openai_key", return_value="sk-test")
    @patch("vaani.main.get_anthropic_key", return_value="sk-test")
    def test_start_foreground(self, mock_ak, mock_ok, mock_load, mock_app_cls):
        config = VaaniConfig(onboarding_completed=True)
        mock_load.return_value = config
        mock_app = MagicMock()
        mock_app_cls.return_value = mock_app

        runner = CliRunner()
        result = runner.invoke(cli, ["start", "--foreground"])
        assert result.exit_code == 0
        mock_app.run.assert_called_once()


# ---------------------------------------------------------------------------
# Prewarm gate — models must be loaded before recording starts
# ---------------------------------------------------------------------------

class TestPrewarmGate:
    """Ensure start_recording blocks until prewarm is done."""

    @pytest.fixture
    def app(self):
        config = VaaniConfig()
        app = VaaniApp(config)
        mock_recorder = MagicMock()
        mock_recorder.current_level = 0.5
        mock_recorder.stop.return_value = np.zeros(16000, dtype=np.float32)
        app._recorder = mock_recorder
        return app

    def test_recording_blocked_until_prewarm_done(self, app):
        """start_recording should not transition to RECORDING while prewarm is pending."""
        app.PREWARM_TIMEOUT = 0.1
        app.start_recording()
        assert app.state.is_idle, "Recording must not start before prewarm completes"

    def test_recording_allowed_after_prewarm(self, app):
        """Once prewarm is signalled, recording should proceed normally."""
        app._prewarm_done.set()
        app.start_recording()
        assert app.state.is_recording

    def test_prewarm_sets_event_on_success(self, app):
        with patch("vaani.audio._load_vad"), \
             patch("vaani.output._load_nlp"):
            app._prewarm()
        assert app._prewarm_done.is_set()

    def test_prewarm_sets_event_on_failure(self, app):
        """Even if a model fails to load, the event fires so the app doesn't hang."""
        with patch("vaani.audio._load_vad", side_effect=RuntimeError("download failed")):
            app._prewarm()
        assert app._prewarm_done.is_set()

    @patch("vaani.output._load_nlp")
    @patch("vaani.audio._load_vad")
    def test_prewarm_loads_vad_and_nlp(self, mock_vad, mock_nlp, app):
        app._prewarm()
        mock_vad.assert_called_once()
        mock_nlp.assert_called_once()

    def test_delayed_prewarm_unblocks_recording(self, app):
        """Simulate prewarm finishing after a short delay — recording should proceed."""
        def delayed_set():
            import time
            time.sleep(0.05)
            app._prewarm_done.set()

        threading.Thread(target=delayed_set, daemon=True).start()
        app.start_recording()
        assert app.state.is_recording

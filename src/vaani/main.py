"""Entry point: CLI commands and pipeline orchestration."""

import logging
import logging.handlers
import os
import threading
import time
from pathlib import Path
from typing import Optional

import click
import numpy as np

from vaani.config import (
    VAANI_DIR,
    VaaniConfig,
    get_anthropic_key,
    get_openai_key,
    load_config,
    save_config,
    set_api_key,
)
from vaani.state import AppState, StateMachine

logger = logging.getLogger("vaani")


def setup_logging(config: VaaniConfig) -> None:
    """Configure rotating file + stderr logging."""
    log_path = Path(config.log_file)
    log_path.parent.mkdir(parents=True, exist_ok=True)

    handler = logging.handlers.RotatingFileHandler(
        log_path,
        maxBytes=config.log_max_bytes,
        backupCount=config.log_backup_count,
    )
    handler.setFormatter(
        logging.Formatter("%(asctime)s %(levelname)s %(name)s: %(message)s")
    )

    stderr = logging.StreamHandler()
    stderr.setLevel(logging.WARNING)
    stderr.setFormatter(logging.Formatter("%(levelname)s: %(message)s"))

    root = logging.getLogger("vaani")
    root.setLevel(logging.INFO)
    root.addHandler(handler)
    root.addHandler(stderr)


class VaaniApp:
    """Orchestrates the full voice-to-text pipeline."""

    def __init__(self, config: VaaniConfig) -> None:
        self.config = config
        self.state = StateMachine()
        self.menubar = None
        self._recorder = None
        self._recorder_device = None
        self._history = None
        self._hotkey_listener = None

    def _get_recorder(self):
        from vaani.audio import AudioRecorder
        if self._recorder is None:
            self._recorder = AudioRecorder(
                sample_rate=self.config.sample_rate,
                device=self._recorder_device
            )
        return self._recorder

    def _get_history(self):
        from vaani.storage import HistoryStore
        if self._history is None:
            self._history = HistoryStore()
        return self._history

    def start_recording(self) -> None:
        """Begin recording audio from the microphone."""
        if not self.state.transition(AppState.RECORDING):
            logger.warning("Cannot start recording in state: %s", self.state.state.value)
            if self.menubar and self.state.is_processing:
                self.menubar.show_notification("Vaani", "Still processing previous recording...")
            return

        if self.menubar:
            self.menubar.update_state(AppState.RECORDING)
            if self.config.sounds_enabled:
                self.menubar.play_sound("Tink")


        recorder = self._get_recorder()
        recorder.start()

        # Auto-stop after max recording time
        def _auto_stop():
            time.sleep(self.config.max_recording_seconds)
            if self.state.is_recording:
                logger.warning("Max recording length reached, auto-stopping")
                if self.menubar:
                    self.menubar.show_notification("Vaani", "Max recording length reached")
                self.stop_recording()

        threading.Thread(target=_auto_stop, daemon=True).start()

    def stop_recording(self) -> None:
        """Stop recording and process the audio."""
        if not self.state.is_recording:
            return

        recorder = self._get_recorder()
        audio = recorder.stop()

        if self.config.sounds_enabled and self.menubar:
            self.menubar.play_sound("Pop")

        if not self.state.transition(AppState.PROCESSING):
            return

        if self.menubar:
            self.menubar.update_state(AppState.PROCESSING)


        # Process in background thread
        threading.Thread(
            target=self._process_audio, args=(audio,), daemon=True
        ).start()

    def cancel_recording(self) -> None:
        """Cancel recording, discard audio."""
        if not self.state.is_recording:
            return

        recorder = self._get_recorder()
        recorder.cancel()
        self.state.transition(AppState.IDLE)

        if self.menubar:
            self.menubar.update_state(AppState.IDLE)
        logger.info("Recording cancelled by user")

    def toggle_recording(self) -> None:
        """Toggle recording on/off (for menu bar click)."""
        if self.state.is_idle:
            self.start_recording()
        elif self.state.is_recording:
            self.stop_recording()

    def _process_audio(self, audio: np.ndarray) -> None:
        """Full processing pipeline: VAD → STT → LLM → paste."""
        try:
            from vaani.audio import process_audio
            from vaani.enhance import enhance
            from vaani.output import paste_text
            from vaani.transcribe import transcribe

            audio_length_secs = len(audio) / self.config.sample_rate

            # Save pending audio in case of failure
            pending_path = VAANI_DIR / "pending" / "last_recording.wav"
            pending_path.parent.mkdir(parents=True, exist_ok=True)
            from vaani.audio import encode_wav
            pending_path.write_bytes(encode_wav(audio, self.config.sample_rate))

            # Audio processing (VAD + gain norm + WAV encode)
            wav_bytes = process_audio(
                audio, self.config.sample_rate, self.config.vad_threshold
            )

            if wav_bytes is None:
                logger.info("No speech detected")
                if self.menubar:
                    self.menubar.show_notification("Vaani", "No speech detected")
                return

            # Transcribe
            raw_text = transcribe(wav_bytes, model=self.config.stt_model)
            if not raw_text.strip() or len(raw_text.strip()) < 3:
                logger.info("Empty or junk transcription: %r", raw_text)
                if self.menubar:
                    self.menubar.show_notification("Vaani", "Could not transcribe audio")
                return

            # Enhance
            enhanced_text = enhance(
                raw_text,
                mode=self.config.active_mode,
                model=self.config.llm_model,
            )

            # Paste at cursor
            paste_text(enhanced_text, self.config.paste_restore_delay_ms)

            # Store in history
            try:
                self._get_history().add(
                    raw_text=raw_text,
                    enhanced_text=enhanced_text,
                    mode=self.config.active_mode,
                    audio_length_secs=audio_length_secs,
                )
            except Exception:
                logger.exception("Failed to store history")

            # Clean up pending audio on success
            if pending_path.exists():
                pending_path.unlink()

        except Exception as e:
            logger.exception("Processing pipeline failed")
            if self.menubar:
                self.menubar.show_notification(
                    "Vaani Error", str(e)[:100]
                )
        finally:
            self.state.transition(AppState.IDLE)
            if self.menubar:
                self.menubar.update_state(AppState.IDLE)

    def _on_mode_change(self, mode: str) -> None:
        self.config.active_mode = mode
        save_config(self.config)

    def _on_microphone_change(self, device_index: Optional[int]) -> None:
        """Handle microphone selection change."""
        self._recorder_device = device_index
        # Reset recorder to use new device on next recording
        self._recorder = None
        if device_index is not None:
            import sounddevice as sd
            device_info = sd.query_devices(device_index)
            device_name = device_info['name'] if isinstance(device_info, dict) else "unknown"
            logger.info("Microphone changed to: %s", device_name)
        else:
            logger.info("Microphone changed to: default")

    def _prewarm(self) -> None:
        """Pre-initialize recorder and VAD model in background to avoid first-use lag."""
        try:
            self._get_recorder()
            from vaani.audio import _load_vad
            _load_vad()
            logger.info("Prewarm complete")
        except Exception:
            logger.exception("Prewarm failed")

    def run(self) -> None:
        """Start the app: hotkey listener + menu bar (main thread)."""
        from vaani.hotkey import HotkeyListener
        from vaani.menubar import VaaniMenuBar

        threading.Thread(target=self._prewarm, daemon=True).start()

        # Start hotkey listener
        self._hotkey_listener = HotkeyListener(
            hotkey=self.config.hotkey,
            on_press=self.start_recording,
            on_release=self.stop_recording,
            on_cancel=self.cancel_recording,
        )
        self._hotkey_listener.start()

        # Menu bar must run on main thread (macOS requirement)
        self.menubar = VaaniMenuBar(
            on_toggle_recording=self.toggle_recording,
            on_mode_change=self._on_mode_change,
            active_mode=self.config.active_mode,
            get_level=lambda: self._get_recorder().current_level,
            on_microphone_change=self._on_microphone_change,
        )

        self.menubar.run()  # Blocks until quit


# --- CLI ---


@click.group()
def cli():
    """Vaani — Voice to polished text, right at your cursor."""
    pass


@cli.command()
def setup():
    """Configure API keys and initial settings."""
    click.echo("Vaani Setup")
    click.echo("=" * 40)
    click.echo()
    click.echo("Vaani sends your audio to OpenAI for transcription")
    click.echo("and to Anthropic for text enhancement.")
    click.echo("Your API keys are stored securely in macOS Keychain.")
    click.echo()

    # OpenAI key
    existing_openai = get_openai_key()
    if existing_openai:
        click.echo(f"OpenAI API key: configured (****{existing_openai[-4:]})")
        if click.confirm("Update OpenAI API key?", default=False):
            key = click.prompt("Enter OpenAI API key", hide_input=True)
            set_api_key("openai_api_key", key.strip())
    else:
        key = click.prompt("Enter OpenAI API key", hide_input=True)
        set_api_key("openai_api_key", key.strip())

    click.echo()

    # Anthropic key
    existing_anthropic = get_anthropic_key()
    if existing_anthropic:
        click.echo(f"Anthropic API key: configured (****{existing_anthropic[-4:]})")
        if click.confirm("Update Anthropic API key?", default=False):
            key = click.prompt("Enter Anthropic API key", hide_input=True)
            set_api_key("anthropic_api_key", key.strip())
    else:
        key = click.prompt("Enter Anthropic API key", hide_input=True)
        set_api_key("anthropic_api_key", key.strip())

    # Save default config
    config = load_config()
    save_config(config)

    click.echo()
    click.echo("Setup complete!")
    click.echo()
    click.echo("Next steps:")
    click.echo("  1. Run: vaani start")
    click.echo("  2. Grant Microphone, Accessibility, and Input Monitoring")
    click.echo("     permissions when prompted (System Settings → Privacy)")
    click.echo(f"  3. Hold {config.hotkey} to record, release to transcribe")


@cli.command()
@click.option("--foreground", is_flag=True, help="Run in foreground (for debugging)")
def start(foreground):
    """Launch the Vaani menu bar app (background by default)."""
    import subprocess

    # Check API keys first
    if not get_openai_key() or not get_anthropic_key():
        click.echo("API keys not configured. Running setup...")
        setup()

    # Kill existing Vaani processes
    try:
        subprocess.run(
            ["pkill", "-f", "vaani start"],
            capture_output=True,
            timeout=5
        )
        time.sleep(1)
    except Exception:
        pass

    # Foreground mode (interactive, for debugging)
    if foreground:
        config = load_config()
        setup_logging(config)
        logger.info("Starting Vaani v%s (foreground)", "0.1.0")
        app = VaaniApp(config)
        app.run()
        return

    # Background mode (daemon-like, default)
    log_path = Path(VAANI_DIR) / "vaani.log"
    log_path.parent.mkdir(parents=True, exist_ok=True)

    try:
        with open(log_path, "w") as log_file:
            process = subprocess.Popen(
                ["vaani", "start", "--foreground"],
                stdout=log_file,
                stderr=subprocess.STDOUT,
                start_new_session=True  # Detaches from current session
            )
        click.echo(f"✓ Vaani started in background (PID: {process.pid})")
        click.echo(f"Logs: tail -f {log_path}")
    except Exception as e:
        click.echo(f"Error starting Vaani: {e}")
        raise SystemExit(1)


if __name__ == "__main__":
    cli()

"""Entry point: CLI commands and pipeline orchestration."""

import logging
import logging.handlers
import os
import threading
import time
from pathlib import Path

import click
import numpy as np

from vaani.config import (
    CONFIG_FILE,
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

    PREWARM_TIMEOUT = 120  # seconds to wait for models before allowing recording

    def __init__(self, config: VaaniConfig) -> None:
        self.config = config
        self._config_mtime: float = CONFIG_FILE.stat().st_mtime if CONFIG_FILE.exists() else 0
        self.state = StateMachine()
        self.menubar = None
        self._recorder = None
        self._history = None
        self._hotkey_listener = None
        self._prewarm_done = threading.Event()

    def _reload_config_if_changed(self) -> None:
        """Reload config from disk only if the file was modified since last check."""
        try:
            mtime = CONFIG_FILE.stat().st_mtime if CONFIG_FILE.exists() else 0
        except OSError:
            return

        if mtime == self._config_mtime:
            return

        self._config_mtime = mtime
        new_config = load_config()

        # Reset recorder if any audio config changed
        if new_config.microphone_device != self.config.microphone_device:
            logger.info(
                "Microphone changed: %s → %s",
                self.config.microphone_device, new_config.microphone_device,
            )
            self._recorder = None
        if new_config.sample_rate != self.config.sample_rate:
            logger.info(
                "Sample rate changed: %s → %s",
                self.config.sample_rate, new_config.sample_rate,
            )
            self._recorder = None

        # Restart hotkey listener if hotkey changed
        if new_config.hotkey != self.config.hotkey:
            logger.info("Hotkey changed: %s → %s", self.config.hotkey, new_config.hotkey)
            self._restart_hotkey_listener(new_config.hotkey)

        self.config = new_config
        logger.info("Config reloaded from disk")

    def _get_recorder(self):
        from vaani.audio import AudioRecorder
        if self._recorder is None:
            self._recorder = AudioRecorder(
                sample_rate=self.config.sample_rate,
                device=self.config.microphone_device,
            )
        return self._recorder

    def _get_history(self):
        from vaani.storage import HistoryStore
        if self._history is None:
            self._history = HistoryStore()
        return self._history

    def _restart_hotkey_listener(self, new_hotkey: str) -> None:
        """Stop and restart the hotkey listener with a new hotkey."""
        if self._hotkey_listener:
            self._hotkey_listener.stop()
        from vaani.hotkey import HotkeyListener
        self._hotkey_listener = HotkeyListener(
            hotkey=new_hotkey,
            on_press=self.start_recording,
            on_release=self.stop_recording,
            on_cancel=self.cancel_recording,
        )
        self._hotkey_listener.start()

    def start_recording(self) -> None:
        """Begin recording audio from the microphone."""
        if not self._prewarm_done.wait(timeout=self.PREWARM_TIMEOUT):
            logger.error("Prewarm did not complete within %ds — refusing to record", self.PREWARM_TIMEOUT)
            if self.menubar:
                self.menubar.show_notification("Vaani", "Models still loading, please wait...")
            return

        self._reload_config_if_changed()

        if not self.state.transition(AppState.RECORDING):
            logger.warning("Cannot start recording in state: %s", self.state.state.value)
            if self.menubar and self.state.is_processing:
                self.menubar.show_notification("Vaani", "Still processing previous recording...")
            return

        if self.menubar:
            self.menubar.update_state(AppState.RECORDING)
            if self.config.sounds_enabled:
                self.menubar.play_sound("record_start")


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
            self.menubar.play_sound("record_stop")

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

    def _prewarm(self) -> None:
        """Pre-initialize recorder, VAD, and NER models before accepting recordings."""
        try:
            self._get_recorder()
            from vaani.audio import _load_vad
            _load_vad()
            from vaani.output import _load_nlp
            _load_nlp()
            logger.info("Prewarm complete — all models loaded")
        except Exception:
            logger.exception("Prewarm failed")
        finally:
            self._prewarm_done.set()

    def run(self) -> None:
        """Start the app: hotkey listener + menu bar (main thread)."""
        import rumps as _rumps
        from vaani.hotkey import HotkeyListener
        from vaani.menubar import VaaniMenuBar

        threading.Thread(target=self._prewarm, daemon=True).start()

        self.menubar = VaaniMenuBar(
            on_toggle_recording=self.toggle_recording,
        )

        @_rumps.events.before_start
        def _start_hotkey():
            self._hotkey_listener = HotkeyListener(
                hotkey=self.config.hotkey,
                on_press=self.start_recording,
                on_release=self.stop_recording,
                on_cancel=self.cancel_recording,
            )
            self._hotkey_listener.start()

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
def settings():
    """Open the Vaani settings panel."""
    from vaani.ui.settings import show_settings
    show_settings()


@cli.command()
@click.option("--foreground", is_flag=True, help="Run in foreground (for debugging)")
def start(foreground):
    """Launch the Vaani menu bar app (background by default)."""
    import subprocess

    config = load_config()

    # Show onboarding on first run (before rumps starts)
    if not config.onboarding_completed:
        from vaani.ui.onboarding import show_onboarding
        completed = show_onboarding()
        if not completed:
            click.echo("Onboarding cancelled. Run 'vaani start' to try again.")
            return
        config = load_config()  # Reload after onboarding saved changes

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
        logger.info("Starting Vaani v%s (foreground)", "0.2.2")
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

"""Onboarding wizard window using pywebview."""

import logging
import threading
from pathlib import Path

logger = logging.getLogger(__name__)

WEB_DIR = Path(__file__).parent / "web"


def show_onboarding() -> bool:
    """Show the onboarding wizard. Returns True if onboarding was completed."""
    import webview

    from vaani.config import load_config
    from vaani.ui.api import VaaniAPI

    api = VaaniAPI()

    window = webview.create_window(
        "Welcome to Vaani",
        url=str(WEB_DIR / "onboarding.html"),
        js_api=api,
        width=640,
        height=680,
        resizable=False,
        on_top=True,
    )

    api._window = window

    def _wait_and_destroy():
        api.close_requested.wait()
        window.destroy()

    threading.Thread(target=_wait_and_destroy, daemon=True).start()

    webview.start()

    config = load_config()
    return config.onboarding_completed

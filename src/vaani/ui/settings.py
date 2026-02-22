"""Settings window — native PyObjC WKWebView, runs in the menu bar process."""

import json
import logging
import threading
from pathlib import Path

import objc
from AppKit import NSApp, NSBackingStoreBuffered, NSWindow
from Foundation import NSMakeRect, NSObject, NSURL
from PyObjCTools import AppHelper
from WebKit import (
    WKUserContentController,
    WKUserScript,
    WKWebView,
    WKWebViewConfiguration,
)

logger = logging.getLogger(__name__)

WEB_DIR = Path(__file__).parent / "web"

# Singleton — one settings window at a time.
_window = None
# prevent prevent GC of ObjC helper objects (PyObjC objects can't hold Python attrs)
_refs = {}

# NSWindowStyleMask: titled | closable | miniaturizable | resizable
_STYLE = 1 | 2 | 4 | 8

# ---------------------------------------------------------------------------
# JS shim injected at document-start.
# Creates window.pywebview.api.* that proxy to WKScriptMessageHandler,
# then fires 'pywebviewready' so the existing ready() helper in app.js works.
# ---------------------------------------------------------------------------
_BRIDGE_JS = """
(function() {
    var cbs = {}, seq = 0;
    window._resolveCallback = function(id, result) {
        var fn = cbs[id];
        if (fn) { delete cbs[id]; fn(result); }
    };
    function m(name) {
        return function() {
            var args = [].slice.call(arguments);
            return new Promise(function(resolve) {
                var id = String(++seq);
                cbs[id] = resolve;
                window.webkit.messageHandlers.api.postMessage({
                    method: name, args: args, callbackId: id
                });
            });
        };
    }
    var api = {};
    ['close_window','get_config','save_config','get_api_keys_status',
     'set_api_key','list_microphones','start_mic_test','get_mic_level',
     'stop_mic_test','get_hotkey','set_hotkey','check_permissions',
     'complete_onboarding','get_version','open_log_file','open_config_dir'
    ].forEach(function(n) { api[n] = m(n); });
    window.pywebview = { api: api };
    window.dispatchEvent(new Event('pywebviewready'));
})();
"""


# ---------------------------------------------------------------------------
# ObjC helpers
# ---------------------------------------------------------------------------
class _MessageHandler(NSObject):
    """WKScriptMessageHandler — routes JS calls to VaaniAPI methods."""

    def init(self):
        self = objc.super(_MessageHandler, self).init()
        return self

    def userContentController_didReceiveScriptMessage_(self, controller, message):
        body = message.body()
        name = body["method"]
        args = body.get("args", [])
        cid = body.get("callbackId")

        def _call():
            fn = getattr(self.api, name, None)
            try:
                result = fn(*args) if fn else {"error": f"unknown method: {name}"}
            except Exception as exc:
                logger.exception("API call %s failed", name)
                result = {"error": str(exc)}

            if cid and self.webview:
                js = f"window._resolveCallback('{cid}',{json.dumps(result)})"
                AppHelper.callAfter(
                    self.webview.evaluateJavaScript_completionHandler_, js, None
                )

        # Run off the main thread so the UI stays responsive.
        threading.Thread(target=_call, daemon=True).start()


class _WindowDelegate(NSObject):
    """Clears the singleton when the window is closed."""

    def windowWillClose_(self, notification):
        global _window
        _window = None
        _refs.clear()


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------
def open_settings():
    """Open (or bring to front) the settings window inside the running app.

    Uses a native NSWindow + WKWebView — no subprocess, no pywebview.
    """
    global _window

    if _window is not None:
        _window.makeKeyAndOrderFront_(None)
        NSApp.activateIgnoringOtherApps_(True)
        return

    from vaani.ui.api import VaaniAPI

    api = VaaniAPI()

    # --- WKWebView configuration with JS bridge ---
    wk_config = WKWebViewConfiguration.alloc().init()
    uc = WKUserContentController.alloc().init()

    script = WKUserScript.alloc().initWithSource_injectionTime_forMainFrameOnly_(
        _BRIDGE_JS, 0, True  # 0 = WKUserScriptInjectionTimeAtDocumentStart
    )
    uc.addUserScript_(script)
    wk_config.setUserContentController_(uc)

    # --- NSWindow ---
    frame = NSMakeRect(0, 0, 600, 500)
    win = NSWindow.alloc().initWithContentRect_styleMask_backing_defer_(
        frame, _STYLE, NSBackingStoreBuffered, False
    )
    win.setTitle_("Vaani Settings")
    win.setMinSize_((500, 400))
    win.center()

    delegate = _WindowDelegate.alloc().init()
    win.setDelegate_(delegate)

    # --- WKWebView ---
    wv = WKWebView.alloc().initWithFrame_configuration_(
        win.contentView().bounds(), wk_config
    )
    wv.setAutoresizingMask_(18)  # NSViewWidthSizable | NSViewHeightSizable

    handler = _MessageHandler.alloc().init()
    handler.api = api
    handler.webview = wv
    uc.addScriptMessageHandler_name_(handler, "api")

    # prevent GC of ObjC helpers (can't set attrs on ObjC objects directly)
    _refs["delegate"] = delegate
    _refs["handler"] = handler

    # --- Load HTML ---
    html_url = NSURL.fileURLWithPath_(str(WEB_DIR / "settings.html"))
    wv.loadFileURL_allowingReadAccessToURL_(
        html_url, NSURL.fileURLWithPath_(str(WEB_DIR) + "/")
    )

    win.contentView().addSubview_(wv)
    win.makeKeyAndOrderFront_(None)
    NSApp.activateIgnoringOtherApps_(True)

    _window = win
    api._window = win


def show_settings():
    """Standalone mode (for ``vaani settings`` CLI) — uses pywebview."""
    import webview

    from vaani.ui.api import VaaniAPI

    api = VaaniAPI()
    window = webview.create_window(
        "Vaani Settings",
        url=str(WEB_DIR / "settings.html"),
        js_api=api,
        width=600,
        height=500,
        resizable=True,
        min_size=(500, 400),
        background_color="#f5f5f7",
    )
    api._window = window
    webview.start()


if __name__ == "__main__":
    show_settings()

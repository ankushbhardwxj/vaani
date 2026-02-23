"""Settings window — native NSWindow + WKWebView for in-process use (menu bar),
pywebview for standalone CLI use."""

import inspect
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

_window = None
_refs = {}

_STYLE = 1 | 2 | 4 | 8  # titled | closable | miniaturizable | resizable

# ---------------------------------------------------------------------------
# JS bridge — auto-generated from VaaniAPI public methods
# ---------------------------------------------------------------------------

_BRIDGE_JS_TEMPLATE = """\
(function() {{
    var cbs = {{}}, seq = 0;
    window._resolveCallback = function(id, result) {{
        var fn = cbs[id];
        if (fn) {{ delete cbs[id]; fn(result); }}
    }};
    function m(name) {{
        return function() {{
            var args = [].slice.call(arguments);
            return new Promise(function(resolve) {{
                var id = String(++seq);
                cbs[id] = resolve;
                window.webkit.messageHandlers.api.postMessage({{
                    method: name, args: args, callbackId: id
                }});
            }});
        }};
    }}
    var api = {{}};
    {method_list}.forEach(function(n) {{ api[n] = m(n); }});
    window.pywebview = {{ api: api }};
    window.dispatchEvent(new Event('pywebviewready'));
}})();
"""


def _get_api_methods() -> list[str]:
    """Discover all public methods on VaaniAPI via introspection."""
    from vaani.ui.api import VaaniAPI
    return [
        name for name, _ in inspect.getmembers(VaaniAPI, predicate=inspect.isfunction)
        if not name.startswith("_")
    ]


def _build_bridge_js() -> str:
    methods = _get_api_methods()
    return _BRIDGE_JS_TEMPLATE.format(method_list=json.dumps(methods))


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

        threading.Thread(target=_call, daemon=True).start()


class _WindowDelegate(NSObject):
    """Clears the singleton when the window is closed."""

    def windowWillClose_(self, notification):
        global _window
        _window = None
        _refs.clear()


# ---------------------------------------------------------------------------
# In-process settings (menu bar) — native NSWindow + WKWebView
# ---------------------------------------------------------------------------

def open_settings():
    """Open (or bring to front) the settings window inside the running app."""
    global _window

    if _window is not None:
        _window.makeKeyAndOrderFront_(None)
        NSApp.activateIgnoringOtherApps_(True)
        return

    from vaani.ui.api import VaaniAPI

    api = VaaniAPI()

    wk_config = WKWebViewConfiguration.alloc().init()
    uc = WKUserContentController.alloc().init()

    bridge_js = _build_bridge_js()
    script = WKUserScript.alloc().initWithSource_injectionTime_forMainFrameOnly_(
        bridge_js, 0, True  # 0 = AtDocumentStart
    )
    uc.addUserScript_(script)
    wk_config.setUserContentController_(uc)

    frame = NSMakeRect(0, 0, 600, 500)
    win = NSWindow.alloc().initWithContentRect_styleMask_backing_defer_(
        frame, _STYLE, NSBackingStoreBuffered, False
    )
    win.setTitle_("Vaani Settings")
    win.setMinSize_((500, 400))
    win.center()

    delegate = _WindowDelegate.alloc().init()
    win.setDelegate_(delegate)

    wv = WKWebView.alloc().initWithFrame_configuration_(
        win.contentView().bounds(), wk_config
    )
    wv.setAutoresizingMask_(18)  # NSViewWidthSizable | NSViewHeightSizable

    handler = _MessageHandler.alloc().init()
    handler.api = api
    handler.webview = wv
    uc.addScriptMessageHandler_name_(handler, "api")

    _refs["delegate"] = delegate
    _refs["handler"] = handler

    html_url = NSURL.fileURLWithPath_(str(WEB_DIR / "settings.html"))
    wv.loadFileURL_allowingReadAccessToURL_(
        html_url, NSURL.fileURLWithPath_(str(WEB_DIR) + "/")
    )

    win.contentView().addSubview_(wv)
    win.makeKeyAndOrderFront_(None)
    NSApp.activateIgnoringOtherApps_(True)

    _window = win
    api._window = win

    def _wait_and_close():
        api.close_requested.wait()
        AppHelper.callAfter(win.close)

    threading.Thread(target=_wait_and_close, daemon=True).start()


# ---------------------------------------------------------------------------
# Standalone settings (CLI) — pywebview
# ---------------------------------------------------------------------------

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

    def _wait_and_destroy():
        api.close_requested.wait()
        window.destroy()

    threading.Thread(target=_wait_and_destroy, daemon=True).start()

    webview.start()


if __name__ == "__main__":
    show_settings()

#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/src-tauri"

echo "==> Building Vaani.app (release)..."
cargo tauri build

APP_SRC="target/release/bundle/macos/Vaani.app"
APP_DST="/Applications/Vaani.app"

if [ ! -d "$APP_SRC" ]; then
  echo "Build artifact not found at $APP_SRC" >&2
  exit 1
fi

echo "==> Installing to $APP_DST"
rm -rf "$APP_DST"
cp -R "$APP_SRC" "$APP_DST"

# macOS gatekeeper: unsigned builds get a quarantine bit on first download/copy.
# Strip it so the app opens without the "unidentified developer" gate.
xattr -dr com.apple.quarantine "$APP_DST" 2>/dev/null || true

mkdir -p "$HOME/.vaani"
CFG="$HOME/.vaani/config.yaml"
if [ ! -f "$CFG" ]; then
  cat > "$CFG" <<'YAML'
hotkey: alt
active_mode: professional
openai_api_key: null
anthropic_api_key: null
YAML
  echo "==> Wrote starter config at $CFG"
fi

echo "==> Launching Vaani..."
open -a "/Applications/Vaani.app"

cat <<'EOF'

────────────────────────────────────────────────────────
  Vaani is installed.

  REQUIRED — grant permissions manually:
    System Settings → Privacy & Security → Microphone     → enable Vaani
    System Settings → Privacy & Security → Accessibility  → enable Vaani

  Then edit ~/.vaani/config.yaml and set:
    openai_api_key: sk-...
    anthropic_api_key: sk-ant-...

  Quit and relaunch Vaani after granting permissions.
────────────────────────────────────────────────────────
EOF

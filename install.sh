#!/usr/bin/env bash
set -euo pipefail

BOLD='\033[1m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
RESET='\033[0m'

info()  { printf "${BOLD}%s${RESET}\n" "$*"; }
ok()    { printf "${GREEN}✓ %s${RESET}\n" "$*"; }
warn()  { printf "${YELLOW}⚠ %s${RESET}\n" "$*"; }
fail()  { printf "${RED}✗ %s${RESET}\n" "$*"; exit 1; }

LINK_DIR="/usr/local/bin"

# --- Preflight ---

info "Installing Vaani..."
echo

command -v python3 >/dev/null 2>&1 || fail "python3 not found. Install Python 3.10+ first."
python3 -m pip --version >/dev/null 2>&1 || fail "pip not found. Install pip first: python3 -m ensurepip"

PYTHON_VERSION=$(python3 -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")')
MAJOR=$(echo "$PYTHON_VERSION" | cut -d. -f1)
MINOR=$(echo "$PYTHON_VERSION" | cut -d. -f2)
if [ "$MAJOR" -lt 3 ] || { [ "$MAJOR" -eq 3 ] && [ "$MINOR" -lt 10 ]; }; then
    fail "Python 3.10+ required (found $PYTHON_VERSION)"
fi
ok "Python $PYTHON_VERSION"

# --- Install ---

info "Installing vaani package..."
python3 -m pip install vaani || fail "pip install failed"
ok "Package installed"

# --- Fix SSL certs (macOS python.org installs lack root certs) ---
# pip bundles its own SSL so the install above works, but Python's stdlib
# urllib does not — torch.hub and spacy download will fail without this.
# certifi is a direct dependency of vaani, so it's guaranteed to be installed.

if ! python3 -c "import urllib.request; urllib.request.urlopen('https://github.com')" 2>/dev/null; then
    CERT_PATH=$(python3 -c "import certifi; print(certifi.where())")
    export SSL_CERT_FILE="$CERT_PATH"
    ok "Configured SSL certificates ($CERT_PATH)"
fi

# --- Locate the binary ---

VAANI_BIN=$(python3 -c "
import sysconfig, os
scripts = sysconfig.get_path('scripts')
candidate = os.path.join(scripts, 'vaani')
if os.path.exists(candidate):
    print(candidate)
else:
    user_scripts = sysconfig.get_path('scripts', scheme='posix_user')
    user_candidate = os.path.join(user_scripts, 'vaani')
    if os.path.exists(user_candidate):
        print(user_candidate)
" 2>/dev/null)

if [ -z "${VAANI_BIN:-}" ]; then
    VAANI_BIN=$(command -v vaani 2>/dev/null || true)
fi

if [ -z "${VAANI_BIN:-}" ]; then
    fail "Could not find the vaani binary after install. Try: pip3 install --user vaani"
fi

ok "Found vaani at $VAANI_BIN"

# --- Symlink to PATH if needed ---

if command -v vaani >/dev/null 2>&1; then
    ok "vaani is already on PATH"
else
    info "Linking vaani to $LINK_DIR..."
    sudo mkdir -p "$LINK_DIR"
    sudo ln -sf "$VAANI_BIN" "$LINK_DIR/vaani"
    ok "Linked to $LINK_DIR/vaani"
fi

# --- Download spaCy model ---

if python3 -c "import spacy; spacy.load('en_core_web_sm')" 2>/dev/null; then
    ok "spaCy model already installed"
else
    info "Downloading spaCy language model..."
    python3 -m spacy download en_core_web_sm -q || warn "spaCy model download failed (NER name formatting will be skipped)"
    ok "spaCy model installed"
fi

# --- Done ---

echo
ok "Vaani installed successfully!"
echo
info "Get started:"
echo "  vaani start"
echo

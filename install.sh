#!/usr/bin/env bash
# Vaani installer
# Usage: curl -fsSL https://ankushbhardwxj.github.io/vaani/install.sh | bash
set -e

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'

echo ""
echo "  वाणी  Vaani installer"
echo "  ─────────────────────"
echo ""

# Check Python 3.10+
if ! command -v python3 &>/dev/null; then
    echo -e "${RED}Error:${NC} Python 3 not found."
    echo "  Install it: brew install python@3.12"
    exit 1
fi

PY_MINOR=$(python3 -c 'import sys; print(sys.version_info.minor)')
PY_MAJOR=$(python3 -c 'import sys; print(sys.version_info.major)')
if [ "$PY_MAJOR" -lt 3 ] || [ "$PY_MINOR" -lt 10 ]; then
    echo -e "${RED}Error:${NC} Python 3.10+ required (you have 3.${PY_MINOR})"
    echo "  Install it: brew install python@3.12"
    exit 1
fi

echo -e "  ${GREEN}✓${NC} Python 3.${PY_MINOR} found"

# Install
echo "  Installing Vaani from PyPI..."
pip install -q --upgrade vaani
echo -e "  ${GREEN}✓${NC} Vaani installed"

# Download spaCy model (needed for NER name formatting)
echo "  Downloading spaCy language model..."
python3 -m spacy download en_core_web_sm -q
echo -e "  ${GREEN}✓${NC} spaCy model ready"

echo ""
echo "  Running setup (you'll be prompted for your API keys)..."
echo ""

vaani setup

echo ""
echo -e "  ${GREEN}Done!${NC} Start Vaani with:"
echo ""
echo "    vaani start"
echo ""
echo -e "  ${YELLOW}Note:${NC} On first launch, grant Microphone, Accessibility,"
echo "  and Input Monitoring in System Settings → Privacy & Security."
echo ""

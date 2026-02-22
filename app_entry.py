"""Entry point for standalone Vaani.app build."""

import sys
sys.argv = [sys.argv[0], "start"]

from vaani.main import cli
cli()

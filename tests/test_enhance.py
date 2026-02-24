"""Tests for vaani.enhance — prompt assembly + mocked Anthropic client."""

from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest


# ---------------------------------------------------------------------------
# Prompt assembly
# ---------------------------------------------------------------------------

class TestPromptAssembly:
    def test_loads_project_prompt(self, tmp_vaani_dir, monkeypatch):
        from vaani.enhance import _load_prompt_file, _BUNDLED_PROMPTS

        # Write a project prompt
        project_dir = Path(_BUNDLED_PROMPTS)
        project_dir.mkdir(parents=True, exist_ok=True)
        (project_dir / "test_prompt.txt").write_text("project prompt content")

        result = _load_prompt_file("test_prompt.txt")
        assert result == "project prompt content"

    def test_user_override_wins(self, tmp_vaani_dir):
        from vaani.enhance import _load_prompt_file, _BUNDLED_PROMPTS

        # Write both project and user prompts
        project_dir = Path(_BUNDLED_PROMPTS)
        project_dir.mkdir(parents=True, exist_ok=True)
        (project_dir / "override.txt").write_text("project version")

        user_dir = tmp_vaani_dir / "prompts"
        user_dir.mkdir(parents=True, exist_ok=True)
        (user_dir / "override.txt").write_text("user version")

        result = _load_prompt_file("override.txt")
        assert result == "user version"

    def test_fallback_when_no_prompts(self, tmp_vaani_dir, monkeypatch):
        from vaani.enhance import _build_system_prompt

        # Point project prompts to nonexistent dir
        monkeypatch.setattr(
            "vaani.enhance._BUNDLED_PROMPTS", tmp_vaani_dir / "nonexistent"
        )
        result = _build_system_prompt("minimal")
        assert "enhance" in result.lower() or "transcription" in result.lower()

    def test_mode_prompt_included(self, tmp_vaani_dir, monkeypatch):
        from vaani.enhance import _build_system_prompt

        monkeypatch.setattr(
            "vaani.enhance._BUNDLED_PROMPTS", tmp_vaani_dir / "prompts"
        )
        modes_dir = tmp_vaani_dir / "prompts" / "modes"
        modes_dir.mkdir(parents=True, exist_ok=True)
        (modes_dir / "code.txt").write_text("Format for code context")

        result = _build_system_prompt("code")
        assert "code context" in result.lower()


# ---------------------------------------------------------------------------
# Mocked Anthropic streaming
# ---------------------------------------------------------------------------

class TestEnhance:
    def test_missing_key_raises_runtime_error(self, mock_keyring):
        from vaani.enhance import enhance
        with pytest.raises(RuntimeError, match="Anthropic API key not found"):
            enhance("hello world")

    def test_empty_transcription_returned_as_is(self, mock_keyring):
        from vaani.enhance import enhance
        mock_keyring.set_password("vaani", "anthropic_api_key", "sk-test")
        result = enhance("  ")
        assert result == "  "

    @patch("vaani.enhance.anthropic.Anthropic")
    def test_streamed_response_assembled(self, mock_anthropic_cls, mock_keyring):
        from vaani.enhance import enhance

        mock_keyring.set_password("vaani", "anthropic_api_key", "sk-test")

        # Set up streaming mock
        mock_stream = MagicMock()
        mock_stream.__enter__ = MagicMock(return_value=mock_stream)
        mock_stream.__exit__ = MagicMock(return_value=False)
        mock_stream.text_stream = iter(["Hello", ", ", "world!"])

        mock_client = MagicMock()
        mock_client.messages.stream.return_value = mock_stream
        mock_anthropic_cls.return_value = mock_client

        result = enhance("hello world", mode="minimal")
        assert result == "Hello, world!"

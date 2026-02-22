"""Tests for vaani.transcribe — mocked OpenAI Whisper wrapper."""

from unittest.mock import MagicMock, patch

import pytest


class TestTranscribe:
    def test_missing_key_raises_runtime_error(self, mock_keyring):
        from vaani.transcribe import transcribe
        with pytest.raises(RuntimeError, match="OpenAI API key not found"):
            transcribe(b"fake wav bytes")

    @patch("vaani.transcribe.OpenAI")
    def test_correct_api_call(self, mock_openai_cls, mock_keyring):
        from vaani.transcribe import transcribe

        mock_keyring.set_password("vaani", "openai_api_key", "sk-test")

        mock_response = MagicMock()
        mock_response.text = "  Hello world  "
        mock_client = MagicMock()
        mock_client.audio.transcriptions.create.return_value = mock_response
        mock_openai_cls.return_value = mock_client

        result = transcribe(b"fake wav", model="whisper-1")
        assert result == "Hello world"

        # Verify the API was called with correct params
        call_kwargs = mock_client.audio.transcriptions.create.call_args
        assert call_kwargs.kwargs["model"] == "whisper-1"

    @patch("vaani.transcribe.OpenAI")
    def test_uses_default_model(self, mock_openai_cls, mock_keyring):
        from vaani.transcribe import transcribe

        mock_keyring.set_password("vaani", "openai_api_key", "sk-test")

        mock_response = MagicMock()
        mock_response.text = "text"
        mock_client = MagicMock()
        mock_client.audio.transcriptions.create.return_value = mock_response
        mock_openai_cls.return_value = mock_client

        transcribe(b"fake wav")
        call_kwargs = mock_client.audio.transcriptions.create.call_args
        assert call_kwargs.kwargs["model"] == "gpt-4o-mini-transcribe"

    @patch("vaani.transcribe.OpenAI")
    def test_strips_whitespace(self, mock_openai_cls, mock_keyring):
        from vaani.transcribe import transcribe

        mock_keyring.set_password("vaani", "openai_api_key", "sk-test")

        mock_response = MagicMock()
        mock_response.text = "\n  transcribed text  \n"
        mock_client = MagicMock()
        mock_client.audio.transcriptions.create.return_value = mock_response
        mock_openai_cls.return_value = mock_client

        result = transcribe(b"wav")
        assert result == "transcribed text"

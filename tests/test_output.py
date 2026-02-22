"""Tests for vaani.output — NER formatting + clipboard mocking."""

from unittest.mock import MagicMock, call, patch

import pytest


# ---------------------------------------------------------------------------
# NER name formatting
# ---------------------------------------------------------------------------

class _FakeEntity:
    def __init__(self, text, label, start_char, end_char):
        self.text = text
        self.label_ = label
        self.start_char = start_char
        self.end_char = end_char


class _FakeDoc:
    def __init__(self, text, entities):
        self.text = text
        self.ents = entities


@pytest.fixture
def mock_nlp(monkeypatch):
    """Patch the spaCy NER model with a fake returning predetermined entities."""
    fake_nlp = MagicMock()
    monkeypatch.setattr("vaani.output._nlp", fake_nlp)
    return fake_nlp


class TestFormatNamesWithAt:
    def test_adds_at_prefix_to_person(self, mock_nlp):
        from vaani.output import _format_names_with_at

        text = "Please ask John about it"
        mock_nlp.return_value = _FakeDoc(text, [
            _FakeEntity("John", "PERSON", 11, 15),
        ])
        result = _format_names_with_at(text)
        assert result == "Please ask @John about it"

    def test_skips_existing_at(self, mock_nlp):
        from vaani.output import _format_names_with_at

        text = "Talk to @Jane today"
        mock_nlp.return_value = _FakeDoc(text, [
            _FakeEntity("@Jane", "PERSON", 8, 13),
        ])
        result = _format_names_with_at(text)
        # Should not become @@Jane
        assert "@@" not in result

    def test_no_double_prefix(self, mock_nlp):
        from vaani.output import _format_names_with_at

        text = "Ask @Bob and Carol"
        mock_nlp.return_value = _FakeDoc(text, [
            _FakeEntity("@Bob", "PERSON", 4, 8),
            _FakeEntity("Carol", "PERSON", 13, 18),
        ])
        result = _format_names_with_at(text)
        assert result.count("@@") == 0
        assert "@Carol" in result

    def test_non_person_entities_ignored(self, mock_nlp):
        from vaani.output import _format_names_with_at

        text = "Meeting at Google"
        mock_nlp.return_value = _FakeDoc(text, [
            _FakeEntity("Google", "ORG", 11, 17),
        ])
        result = _format_names_with_at(text)
        assert result == "Meeting at Google"

    def test_graceful_fallback_when_nlp_none(self, monkeypatch):
        from vaani.output import _format_names_with_at
        monkeypatch.setattr("vaani.output._nlp", None)
        monkeypatch.setattr("vaani.output._load_nlp", lambda: None)
        result = _format_names_with_at("Hello John")
        assert result == "Hello John"

    def test_exception_returns_original(self, mock_nlp):
        from vaani.output import _format_names_with_at
        mock_nlp.side_effect = RuntimeError("model error")
        result = _format_names_with_at("Hello John")
        assert result == "Hello John"


# ---------------------------------------------------------------------------
# Clipboard operations
# ---------------------------------------------------------------------------

class TestClipboard:
    @patch("vaani.output.subprocess.run")
    def test_get_clipboard(self, mock_run):
        from vaani.output import _get_clipboard
        mock_run.return_value = MagicMock(stdout="clipboard text")
        result = _get_clipboard()
        assert result == "clipboard text"

    @patch("vaani.output.subprocess.run")
    def test_set_clipboard(self, mock_run):
        from vaani.output import _set_clipboard
        _set_clipboard("new text")
        mock_run.assert_called_once()
        assert mock_run.call_args.kwargs["input"] == "new text"


# ---------------------------------------------------------------------------
# paste_text integration (NER + clipboard)
# ---------------------------------------------------------------------------

class TestPasteText:
    @patch("vaani.output.Controller")
    @patch("vaani.output.subprocess.run")
    @patch("vaani.output.time.sleep")
    def test_paste_saves_and_restores_clipboard(self, mock_sleep, mock_run, mock_ctrl, mock_nlp):
        from vaani.output import paste_text

        mock_nlp.return_value = _FakeDoc("hello", [])
        mock_run.return_value = MagicMock(stdout="original")

        paste_text("hello", restore_delay_ms=10)

        # Should have called pbpaste (get) and pbcopy (set) multiple times
        assert mock_run.call_count >= 3  # get original + set new + restore

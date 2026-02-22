"""Tests for vaani.storage — HistoryStore with real SQLite + Fernet encryption."""

import pytest
from cryptography.fernet import Fernet

from vaani.storage import HistoryStore


@pytest.fixture
def store(tmp_vaani_dir, fernet_key, mock_keyring):
    """Create a HistoryStore with a real Fernet key in a temp directory."""
    # Pre-seed the mock keyring with a fernet key
    mock_keyring.set_password("vaani", "fernet_key", fernet_key.decode())
    db_path = tmp_vaani_dir / "history.db"
    s = HistoryStore(db_path=db_path)
    yield s
    s.close()


class TestHistoryStore:
    def test_add_and_retrieve(self, store):
        store.add("hello raw", "hello enhanced", "cleanup", audio_length_secs=2.5)
        records = store.get_recent(limit=10)
        assert len(records) == 1
        assert records[0]["raw"] == "hello raw"
        assert records[0]["enhanced"] == "hello enhanced"
        assert records[0]["mode"] == "cleanup"
        assert records[0]["audio_length_secs"] == 2.5

    def test_ordering_newest_first(self, store):
        store.add("first", "first_e", "cleanup")
        store.add("second", "second_e", "cleanup")
        records = store.get_recent(limit=10)
        assert records[0]["raw"] == "second"
        assert records[1]["raw"] == "first"

    def test_limit(self, store):
        for i in range(5):
            store.add(f"raw{i}", f"enh{i}", "cleanup")
        records = store.get_recent(limit=3)
        assert len(records) == 3

    def test_encryption_is_real(self, store):
        """Raw DB column should NOT contain plaintext."""
        store.add("secret message", "enhanced secret", "cleanup")
        store._ensure_initialized()
        row = store._conn.execute(
            "SELECT raw_encrypted FROM history LIMIT 1"
        ).fetchone()
        assert b"secret message" not in row[0]

    def test_wrong_key_skips_record(self, tmp_vaani_dir, mock_keyring):
        """Records encrypted with one key are skipped when decrypted with another."""
        key1 = Fernet.generate_key()
        mock_keyring.set_password("vaani", "fernet_key", key1.decode())
        db_path = tmp_vaani_dir / "history_wrong.db"

        s1 = HistoryStore(db_path=db_path)
        s1.add("secret", "enhanced", "cleanup")
        s1.close()

        # Re-open with a different key
        key2 = Fernet.generate_key()
        mock_keyring.set_password("vaani", "fernet_key", key2.decode())
        s2 = HistoryStore(db_path=db_path)
        records = s2.get_recent(limit=10)
        assert len(records) == 0  # decryption fails, record skipped
        s2.close()

    def test_optional_fields_none(self, store):
        store.add("raw", "enh", "professional")
        records = store.get_recent()
        assert records[0]["audio_length_secs"] is None
        assert records[0]["language"] is None

    def test_close_and_reopen_persistence(self, tmp_vaani_dir, fernet_key, mock_keyring):
        mock_keyring.set_password("vaani", "fernet_key", fernet_key.decode())
        db_path = tmp_vaani_dir / "persist.db"

        s1 = HistoryStore(db_path=db_path)
        s1.add("persist raw", "persist enh", "casual")
        s1.close()

        s2 = HistoryStore(db_path=db_path)
        records = s2.get_recent()
        assert len(records) == 1
        assert records[0]["raw"] == "persist raw"
        s2.close()

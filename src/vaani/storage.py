"""SQLite + Fernet encrypted transcription history."""

import logging
import sqlite3
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

from cryptography.fernet import Fernet

from vaani.config import VAANI_DIR, get_or_create_fernet_key

logger = logging.getLogger(__name__)

DB_PATH = VAANI_DIR / "history.db"


class HistoryStore:
    """Encrypted transcription history stored in SQLite."""

    def __init__(self, db_path: Optional[Path] = None) -> None:
        self._db_path = db_path or DB_PATH
        self._fernet: Optional[Fernet] = None
        self._conn: Optional[sqlite3.Connection] = None

    def _ensure_initialized(self) -> None:
        if self._conn is not None:
            return

        key = get_or_create_fernet_key()
        self._fernet = Fernet(key)

        self._db_path.parent.mkdir(parents=True, exist_ok=True)
        self._conn = sqlite3.connect(str(self._db_path), check_same_thread=False)
        self._conn.execute("""
            CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                mode TEXT NOT NULL,
                raw_encrypted BLOB NOT NULL,
                enhanced_encrypted BLOB NOT NULL,
                audio_length_secs REAL,
                language TEXT
            )
        """)
        self._conn.commit()
        logger.info("History database initialized at %s", self._db_path)

    def add(
        self,
        raw_text: str,
        enhanced_text: str,
        mode: str,
        audio_length_secs: Optional[float] = None,
        language: Optional[str] = None,
    ) -> int:
        """Add a transcription record. Text is encrypted before storage."""
        self._ensure_initialized()

        raw_enc = self._fernet.encrypt(raw_text.encode())
        enhanced_enc = self._fernet.encrypt(enhanced_text.encode())

        cursor = self._conn.execute(
            """INSERT INTO history (timestamp, mode, raw_encrypted, enhanced_encrypted,
               audio_length_secs, language)
               VALUES (?, ?, ?, ?, ?, ?)""",
            (
                datetime.now(timezone.utc).isoformat(),
                mode,
                raw_enc,
                enhanced_enc,
                audio_length_secs,
                language,
            ),
        )
        self._conn.commit()
        logger.info("History record added (id=%d)", cursor.lastrowid)
        return cursor.lastrowid

    def get_recent(self, limit: int = 20) -> list[dict]:
        """Get recent history entries, decrypted."""
        self._ensure_initialized()

        rows = self._conn.execute(
            """SELECT id, timestamp, mode, raw_encrypted, enhanced_encrypted,
               audio_length_secs, language
               FROM history ORDER BY id DESC LIMIT ?""",
            (limit,),
        ).fetchall()

        results = []
        for row in rows:
            try:
                raw = self._fernet.decrypt(row[3]).decode()
                enhanced = self._fernet.decrypt(row[4]).decode()
            except Exception:
                logger.warning("Failed to decrypt record id=%d", row[0])
                continue

            results.append({
                "id": row[0],
                "timestamp": row[1],
                "mode": row[2],
                "raw": raw,
                "enhanced": enhanced,
                "audio_length_secs": row[5],
                "language": row[6],
            })
        return results

    def close(self) -> None:
        if self._conn:
            self._conn.close()
            self._conn = None

//! Encrypted history storage using SQLite + AES-256-GCM.
//!
//! Text fields (`original_text`, `enhanced_text`) are encrypted at rest.
//! Non-sensitive fields (`mode`, `duration_secs`, `timestamp`) are stored in plaintext.
//! Each encryption operation uses a fresh random 12-byte nonce.

use aes_gcm::aead::generic_array::typenum;
use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use rand::RngCore;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::VaaniError;

/// Size of AES-256-GCM nonce in bytes.
const NONCE_SIZE: usize = 12;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A history record as returned from the database (decrypted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub id: i64,
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Raw transcription text.
    pub original_text: String,
    /// Text after Claude enhancement.
    pub enhanced_text: String,
    /// Enhancement mode that was used.
    pub mode: String,
    /// Recording duration in seconds.
    pub duration_secs: f32,
}

/// Input struct for adding a record (no ID or timestamp — the DB assigns those).
#[derive(Debug)]
pub struct NewHistoryRecord<'a> {
    pub original_text: &'a str,
    pub enhanced_text: &'a str,
    pub mode: &'a str,
    pub duration_secs: f32,
}

/// Encrypted history database backed by SQLite.
pub struct HistoryStore {
    conn: Connection,
    cipher: EncryptionCipher,
}

// ---------------------------------------------------------------------------
// EncryptionCipher
// ---------------------------------------------------------------------------

/// Wraps an AES-256-GCM key and provides encrypt/decrypt helpers.
struct EncryptionCipher {
    key: GenericArray<u8, typenum::U32>,
}

impl EncryptionCipher {
    /// Create a new cipher from a 32-byte key.
    fn new(key: &[u8; 32]) -> Self {
        Self {
            key: *GenericArray::from_slice(key),
        }
    }

    /// Encrypt `plaintext` and return `base64(nonce || ciphertext || tag)`.
    fn encrypt(&self, plaintext: &str) -> Result<String, VaaniError> {
        let cipher = Aes256Gcm::new(&self.key);

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| VaaniError::Storage(format!("encryption failed: {e}")))?;

        let mut combined = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);

        Ok(BASE64.encode(&combined))
    }

    /// Decrypt a base64-encoded blob produced by [`encrypt`].
    fn decrypt(&self, ciphertext_b64: &str) -> Result<String, VaaniError> {
        let combined = BASE64
            .decode(ciphertext_b64)
            .map_err(|e| VaaniError::Storage(format!("base64 decode failed: {e}")))?;

        if combined.len() < NONCE_SIZE + 1 {
            return Err(VaaniError::Storage("encrypted data too short".to_string()));
        }

        let (nonce_bytes, ciphertext) = combined.split_at(NONCE_SIZE);
        let nonce = Nonce::from_slice(nonce_bytes);
        let cipher = Aes256Gcm::new(&self.key);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| VaaniError::Storage(format!("decryption failed: {e}")))?;

        String::from_utf8(plaintext)
            .map_err(|e| VaaniError::Storage(format!("decrypted text is not valid UTF-8: {e}")))
    }
}

// ---------------------------------------------------------------------------
// HistoryStore
// ---------------------------------------------------------------------------

impl HistoryStore {
    /// Open (or create) the history database at `db_path`.
    ///
    /// `encryption_key` must be exactly 32 bytes (AES-256 key size).
    pub fn open(db_path: &Path, encryption_key: &[u8; 32]) -> Result<Self, VaaniError> {
        let conn = Connection::open(db_path)
            .map_err(|e| VaaniError::Storage(format!("failed to open database: {e}")))?;

        create_schema(&conn)?;

        tracing::debug!(?db_path, "history database opened");

        Ok(Self {
            conn,
            cipher: EncryptionCipher::new(encryption_key),
        })
    }

    /// Insert a new record. Text fields are encrypted before storage.
    ///
    /// Returns the auto-generated row ID.
    pub fn add(&self, record: &NewHistoryRecord<'_>) -> Result<i64, VaaniError> {
        let enc_original = self.cipher.encrypt(record.original_text)?;
        let enc_enhanced = self.cipher.encrypt(record.enhanced_text)?;

        self.conn
            .execute(
                "INSERT INTO history (original_text, enhanced_text, mode, duration_secs)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    enc_original,
                    enc_enhanced,
                    record.mode,
                    record.duration_secs
                ],
            )
            .map_err(|e| VaaniError::Storage(format!("insert failed: {e}")))?;

        let id = self.conn.last_insert_rowid();
        tracing::debug!(id, mode = record.mode, "history record added");
        Ok(id)
    }

    /// Retrieve the most recent `limit` records, newest first.
    ///
    /// Text fields are decrypted on retrieval.
    pub fn recent(&self, limit: usize) -> Result<Vec<HistoryRecord>, VaaniError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, timestamp, original_text, enhanced_text, mode, duration_secs
                 FROM history ORDER BY timestamp DESC LIMIT ?1",
            )
            .map_err(|e| VaaniError::Storage(format!("query prepare failed: {e}")))?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(RawRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    original_text: row.get(2)?,
                    enhanced_text: row.get(3)?,
                    mode: row.get(4)?,
                    duration_secs: row.get(5)?,
                })
            })
            .map_err(|e| VaaniError::Storage(format!("query failed: {e}")))?;

        rows.map(|r| {
            let raw = r.map_err(|e| VaaniError::Storage(format!("row read failed: {e}")))?;
            self.decrypt_row(raw)
        })
        .collect()
    }

    /// Get a single record by its ID, or `None` if it does not exist.
    pub fn get(&self, id: i64) -> Result<Option<HistoryRecord>, VaaniError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, timestamp, original_text, enhanced_text, mode, duration_secs
                 FROM history WHERE id = ?1",
            )
            .map_err(|e| VaaniError::Storage(format!("query prepare failed: {e}")))?;

        let mut rows = stmt
            .query_map(params![id], |row| {
                Ok(RawRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    original_text: row.get(2)?,
                    enhanced_text: row.get(3)?,
                    mode: row.get(4)?,
                    duration_secs: row.get(5)?,
                })
            })
            .map_err(|e| VaaniError::Storage(format!("query failed: {e}")))?;

        match rows.next() {
            Some(r) => {
                let raw = r.map_err(|e| VaaniError::Storage(format!("row read failed: {e}")))?;
                Ok(Some(self.decrypt_row(raw)?))
            }
            None => Ok(None),
        }
    }

    /// Delete a single record by ID.
    pub fn delete(&self, id: i64) -> Result<(), VaaniError> {
        self.conn
            .execute("DELETE FROM history WHERE id = ?1", params![id])
            .map_err(|e| VaaniError::Storage(format!("delete failed: {e}")))?;

        tracing::debug!(id, "history record deleted");
        Ok(())
    }

    /// Delete every record in the history table.
    pub fn clear(&self) -> Result<(), VaaniError> {
        self.conn
            .execute("DELETE FROM history", [])
            .map_err(|e| VaaniError::Storage(format!("clear failed: {e}")))?;

        tracing::debug!("history cleared");
        Ok(())
    }

    /// Return the total number of history records.
    pub fn count(&self) -> Result<usize, VaaniError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))
            .map_err(|e| VaaniError::Storage(format!("count query failed: {e}")))?;

        Ok(count as usize)
    }

    // -- private helpers --

    /// Decrypt the encrypted text fields of a raw database row.
    fn decrypt_row(&self, raw: RawRow) -> Result<HistoryRecord, VaaniError> {
        Ok(HistoryRecord {
            id: raw.id,
            timestamp: raw.timestamp,
            original_text: self.cipher.decrypt(&raw.original_text)?,
            enhanced_text: self.cipher.decrypt(&raw.enhanced_text)?,
            mode: raw.mode,
            duration_secs: raw.duration_secs,
        })
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Raw (still-encrypted) row read directly from SQLite.
struct RawRow {
    id: i64,
    timestamp: String,
    original_text: String,
    enhanced_text: String,
    mode: String,
    duration_secs: f32,
}

/// Create the history table and index if they don't exist.
fn create_schema(conn: &Connection) -> Result<(), VaaniError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS history (
             id             INTEGER PRIMARY KEY AUTOINCREMENT,
             timestamp      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
             original_text  TEXT NOT NULL,
             enhanced_text  TEXT NOT NULL,
             mode           TEXT NOT NULL,
             duration_secs  REAL NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_history_timestamp ON history(timestamp DESC);",
    )
    .map_err(|e| VaaniError::Storage(format!("schema creation failed: {e}")))
}

/// Derive a 32-byte encryption key from a passphrase using XOR folding.
///
/// This is a **simple** key-derivation function suitable for development.
/// For production use, prefer PBKDF2 or Argon2.
pub fn derive_key(passphrase: &str) -> [u8; 32] {
    let mut key = [0u8; 32];
    for (i, &b) in passphrase.as_bytes().iter().enumerate() {
        key[i % 32] ^= b;
    }
    key
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: create a store in a temp directory with a test key.
    fn test_store() -> (HistoryStore, TempDir) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let db_path = dir.path().join("history.db");
        let key = derive_key("test-passphrase-for-unit-tests!");
        let store = HistoryStore::open(&db_path, &key).expect("failed to open store");
        (store, dir)
    }

    /// Helper: build a sample new record.
    fn sample_record() -> NewHistoryRecord<'static> {
        NewHistoryRecord {
            original_text: "hello world this is a test",
            enhanced_text: "Hello, world! This is a test.",
            mode: "professional",
            duration_secs: 3.5,
        }
    }

    #[test]
    fn open_creates_database() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let db_path = dir.path().join("new.db");
        assert!(!db_path.exists());

        let key = [0u8; 32];
        let _store = HistoryStore::open(&db_path, &key).expect("open should succeed");
        assert!(db_path.exists());
    }

    #[test]
    fn add_and_retrieve_record() {
        let (store, _dir) = test_store();

        let id = store.add(&sample_record()).expect("add should succeed");
        let record = store
            .get(id)
            .expect("get should succeed")
            .expect("record should exist");

        assert_eq!(record.id, id);
        assert_eq!(record.original_text, "hello world this is a test");
        assert_eq!(record.enhanced_text, "Hello, world! This is a test.");
        assert_eq!(record.mode, "professional");
        assert!((record.duration_secs - 3.5).abs() < f32::EPSILON);
        assert!(!record.timestamp.is_empty());
    }

    #[test]
    fn recent_returns_ordered() {
        let (store, _dir) = test_store();

        // Insert three records with slight delay to get distinct timestamps.
        let id1 = store
            .add(&NewHistoryRecord {
                original_text: "first",
                enhanced_text: "First.",
                mode: "minimal",
                duration_secs: 1.0,
            })
            .expect("add 1");
        let id2 = store
            .add(&NewHistoryRecord {
                original_text: "second",
                enhanced_text: "Second.",
                mode: "casual",
                duration_secs: 2.0,
            })
            .expect("add 2");
        let id3 = store
            .add(&NewHistoryRecord {
                original_text: "third",
                enhanced_text: "Third.",
                mode: "funny",
                duration_secs: 3.0,
            })
            .expect("add 3");

        let records = store.recent(10).expect("recent should succeed");
        assert_eq!(records.len(), 3);

        // Newest first (highest id last inserted, timestamps ascending).
        assert_eq!(records[0].id, id3);
        assert_eq!(records[1].id, id2);
        assert_eq!(records[2].id, id1);
    }

    #[test]
    fn recent_respects_limit() {
        let (store, _dir) = test_store();

        for i in 0..5 {
            store
                .add(&NewHistoryRecord {
                    original_text: "text",
                    enhanced_text: "Text.",
                    mode: "minimal",
                    duration_secs: i as f32,
                })
                .expect("add");
        }

        let records = store.recent(2).expect("recent should succeed");
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn delete_removes_record() {
        let (store, _dir) = test_store();

        let id = store.add(&sample_record()).expect("add");
        assert!(store.get(id).expect("get").is_some());

        store.delete(id).expect("delete should succeed");
        assert!(store.get(id).expect("get after delete").is_none());
    }

    #[test]
    fn clear_removes_all() {
        let (store, _dir) = test_store();

        store.add(&sample_record()).expect("add 1");
        store.add(&sample_record()).expect("add 2");
        assert_eq!(store.count().expect("count"), 2);

        store.clear().expect("clear should succeed");
        assert_eq!(store.count().expect("count after clear"), 0);
    }

    #[test]
    fn count_tracks_records() {
        let (store, _dir) = test_store();

        assert_eq!(store.count().expect("count"), 0);
        store.add(&sample_record()).expect("add 1");
        store.add(&sample_record()).expect("add 2");
        store.add(&sample_record()).expect("add 3");
        assert_eq!(store.count().expect("count"), 3);
    }

    #[test]
    fn encrypted_data_is_not_plaintext() {
        let (store, dir) = test_store();
        let db_path = dir.path().join("history.db");

        let plaintext = "super secret transcription content";
        store
            .add(&NewHistoryRecord {
                original_text: plaintext,
                enhanced_text: "Enhanced secret.",
                mode: "professional",
                duration_secs: 1.0,
            })
            .expect("add");

        // Read the raw database file and verify the plaintext does not appear.
        let raw_bytes = fs::read(&db_path).expect("read db file");
        let raw_string = String::from_utf8_lossy(&raw_bytes);
        assert!(
            !raw_string.contains(plaintext),
            "plaintext should not appear in raw database file"
        );
    }

    #[test]
    fn derive_key_produces_32_bytes() {
        assert_eq!(derive_key("short").len(), 32);
        assert_eq!(derive_key("").len(), 32);
        assert_eq!(
            derive_key("a]very/long*passphrase!that#exceeds-32-bytes-easily").len(),
            32
        );
    }

    #[test]
    fn cipher_roundtrip() {
        let key = derive_key("roundtrip-test");
        let cipher = EncryptionCipher::new(&key);

        let original = "The quick brown fox jumps over the lazy dog.";
        let encrypted = cipher.encrypt(original).expect("encrypt");
        let decrypted = cipher.decrypt(&encrypted).expect("decrypt");

        assert_eq!(decrypted, original);
        // Encrypted form should differ from plaintext.
        assert_ne!(encrypted, original);
    }

    #[test]
    fn wrong_key_fails_decryption() {
        let key_a = derive_key("key-alpha");
        let key_b = derive_key("key-bravo");

        let cipher_a = EncryptionCipher::new(&key_a);
        let cipher_b = EncryptionCipher::new(&key_b);

        let encrypted = cipher_a.encrypt("secret message").expect("encrypt");
        let result = cipher_b.decrypt(&encrypted);

        assert!(result.is_err(), "decryption with wrong key should fail");
    }
}

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS settings (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS conversations (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS messages (
                 id              INTEGER PRIMARY KEY AUTOINCREMENT,
                 conversation_id INTEGER NOT NULL REFERENCES conversations(id),
                 role            TEXT NOT NULL,
                 content         TEXT NOT NULL,
                 created_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );",
        )?;
        Ok(Self { conn })
    }

    pub fn get_setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn current_conversation(&self) -> rusqlite::Result<i64> {
        let id: Option<i64> = self
            .conn
            .query_row("SELECT MAX(id) FROM conversations", [], |r| r.get(0))?;
        match id {
            Some(id) => Ok(id),
            None => self.new_conversation(),
        }
    }

    pub fn new_conversation(&self) -> rusqlite::Result<i64> {
        self.conn.execute("INSERT INTO conversations DEFAULT VALUES", [])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// `content` guarda os blocos de conteúdo exatamente como a API devolveu,
    /// para o histórico ser reenviado sem edições.
    pub fn add_message(&self, conversation: i64, role: &str, content: &Value) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO messages (conversation_id, role, content) VALUES (?1, ?2, ?3)",
            params![conversation, role, content.to_string()],
        )?;
        Ok(())
    }

    pub fn messages(&self, conversation: i64) -> rusqlite::Result<Vec<(String, Value)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT role, content FROM messages WHERE conversation_id = ?1 ORDER BY id")?;
        let rows = stmt.query_map(params![conversation], |r| {
            let role: String = r.get(0)?;
            let content: String = r.get(1)?;
            Ok((role, serde_json::from_str(&content).unwrap_or(Value::Null)))
        })?;
        rows.collect()
    }
}

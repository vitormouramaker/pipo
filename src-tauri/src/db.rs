use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

const CURRENT: &str = "current_conversation";

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

    fn conversation_exists(&self, id: i64) -> bool {
        self.conn
            .query_row("SELECT 1 FROM conversations WHERE id = ?1", params![id], |_| Ok(()))
            .is_ok()
    }

    fn message_count(&self, conversation: i64) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1",
            params![conversation],
            |r| r.get(0),
        )
    }

    /// Conversa aberta no chat (guardada nas configurações para sobreviver ao reinício).
    pub fn current_conversation(&self) -> rusqlite::Result<i64> {
        if let Some(id) = self.get_setting(CURRENT).and_then(|s| s.parse().ok()) {
            if self.conversation_exists(id) {
                return Ok(id);
            }
        }
        let latest: Option<i64> = self
            .conn
            .query_row("SELECT MAX(id) FROM conversations", [], |r| r.get(0))?;
        match latest {
            Some(id) => {
                self.set_current(id)?;
                Ok(id)
            }
            None => self.new_conversation(),
        }
    }

    pub fn set_current(&self, id: i64) -> rusqlite::Result<()> {
        self.set_setting(CURRENT, &id.to_string())
    }

    /// Abre uma conversa nova; se a atual ainda está vazia, reaproveita ela.
    pub fn new_conversation(&self) -> rusqlite::Result<i64> {
        if let Some(id) = self.get_setting(CURRENT).and_then(|s| s.parse().ok()) {
            if self.conversation_exists(id) && self.message_count(id)? == 0 {
                return Ok(id);
            }
        }
        self.conn.execute("INSERT INTO conversations DEFAULT VALUES", [])?;
        let id = self.conn.last_insert_rowid();
        self.set_current(id)?;
        Ok(id)
    }

    /// Conversas com mensagens, da mais recente para a mais antiga:
    /// (id, conteúdo da primeira mensagem do usuário, data da última mensagem).
    pub fn conversations(&self) -> rusqlite::Result<Vec<(i64, Value, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id,
                    (SELECT m.content FROM messages m
                      WHERE m.conversation_id = c.id AND m.role = 'user' ORDER BY m.id LIMIT 1),
                    (SELECT MAX(m.created_at) FROM messages m WHERE m.conversation_id = c.id) AS updated
               FROM conversations c
              WHERE EXISTS (SELECT 1 FROM messages m WHERE m.conversation_id = c.id)
              ORDER BY updated DESC, c.id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let first: Option<String> = r.get(1)?;
            Ok((
                r.get(0)?,
                first.and_then(|f| serde_json::from_str(&f).ok()).unwrap_or(Value::Null),
                r.get(2)?,
            ))
        })?;
        rows.collect()
    }

    pub fn delete_conversation(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM messages WHERE conversation_id = ?1", params![id])?;
        self.conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(())
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

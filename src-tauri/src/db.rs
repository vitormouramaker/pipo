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
             );
             CREATE TABLE IF NOT EXISTS usage (
                 id              INTEGER PRIMARY KEY AUTOINCREMENT,
                 conversation_id INTEGER NOT NULL,
                 model           TEXT NOT NULL,
                 input_tokens    INTEGER NOT NULL,
                 cache_write     INTEGER NOT NULL,
                 cache_read      INTEGER NOT NULL,
                 output_tokens   INTEGER NOT NULL,
                 cost            REAL NOT NULL,
                 created_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS briefings (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 day        TEXT NOT NULL,
                 content    TEXT NOT NULL,
                 sources    TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS notes (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 text       TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS reminders (
                 id     INTEGER PRIMARY KEY AUTOINCREMENT,
                 text   TEXT NOT NULL,
                 due_at TEXT NOT NULL,
                 done   INTEGER NOT NULL DEFAULT 0
             );",
        )?;
        // Colunas acrescentadas depois da primeira versão (o erro de coluna repetida é esperado).
        let _ = conn.execute("ALTER TABLE usage ADD COLUMN web_searches INTEGER NOT NULL DEFAULT 0", []);
        Ok(Self { conn })
    }

    /// Acesso direto para os módulos de ações (notas, lembretes).
    pub fn conn(&self) -> &Connection {
        &self.conn
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

    /// O consumo continua registrado mesmo se a conversa for excluída.
    pub fn add_usage(&self, conversation: i64, usage: &crate::usage::Usage) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO usage (conversation_id, model, input_tokens, cache_write, cache_read, output_tokens, cost, web_searches)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                conversation,
                usage.model,
                usage.input as i64,
                usage.cache_write as i64,
                usage.cache_read as i64,
                usage.output as i64,
                usage.cost(),
                usage.web_searches as i64
            ],
        )?;
        Ok(())
    }

    /// (hoje, este mês, total), em horário local.
    pub fn usage_totals(&self) -> rusqlite::Result<(f64, f64, f64)> {
        self.conn.query_row(
            "SELECT
                 COALESCE(SUM(CASE WHEN date(created_at, 'localtime') = date('now', 'localtime') THEN cost END), 0),
                 COALESCE(SUM(CASE WHEN strftime('%Y-%m', created_at, 'localtime') = strftime('%Y-%m', 'now', 'localtime') THEN cost END), 0),
                 COALESCE(SUM(cost), 0)
               FROM usage",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
    }

    /// Por conversa, da mais recente para a mais antiga:
    /// (id, primeira mensagem do usuário, última data, respostas, tokens de entrada, de saída, custo).
    #[allow(clippy::type_complexity)]
    pub fn usage_by_conversation(&self) -> rusqlite::Result<Vec<(i64, Value, String, i64, i64, i64, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT u.conversation_id,
                    (SELECT m.content FROM messages m
                      WHERE m.conversation_id = u.conversation_id AND m.role = 'user' ORDER BY m.id LIMIT 1),
                    MAX(u.created_at) AS updated,
                    COUNT(*),
                    SUM(u.input_tokens + u.cache_write + u.cache_read),
                    SUM(u.output_tokens),
                    SUM(u.cost)
               FROM usage u
              GROUP BY u.conversation_id
              ORDER BY updated DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let first: Option<String> = r.get(1)?;
            Ok((
                r.get(0)?,
                first.and_then(|f| serde_json::from_str(&f).ok()).unwrap_or(Value::Null),
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })?;
        rows.collect()
    }

    pub fn usage_by_model(&self) -> rusqlite::Result<Vec<(String, i64, f64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT model, COUNT(*), SUM(cost) FROM usage GROUP BY model ORDER BY SUM(cost) DESC")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
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

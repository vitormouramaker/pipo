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
            "PRAGMA journal_mode = DELETE;
             PRAGMA synchronous = FULL;
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
             );
             CREATE TABLE IF NOT EXISTS xp_log (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 day        TEXT NOT NULL,
                 kind       TEXT NOT NULL,
                 times      INTEGER NOT NULL DEFAULT 1,
                 xp         INTEGER NOT NULL,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE INDEX IF NOT EXISTS xp_log_kind_day ON xp_log (kind, day);
             CREATE TABLE IF NOT EXISTS activity (
                 day      TEXT NOT NULL,
                 app      TEXT NOT NULL,
                 category TEXT NOT NULL,
                 seconds  INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY (day, app)
             );
             CREATE TABLE IF NOT EXISTS memories (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 text       TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS lol_games (
                 id        INTEGER PRIMARY KEY AUTOINCREMENT,
                 day       TEXT NOT NULL,
                 champion  TEXT NOT NULL,
                 position  TEXT NOT NULL,
                 won       INTEGER NOT NULL,
                 kills     INTEGER NOT NULL,
                 deaths    INTEGER NOT NULL,
                 assists   INTEGER NOT NULL,
                 cs_min    REAL NOT NULL,
                 minutes   REAL NOT NULL,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS achievements (
                 id          TEXT PRIMARY KEY,
                 unlocked_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );",
        )?;
        // Colunas acrescentadas depois da primeira versão (o erro de coluna repetida é esperado).
        let _ = conn.execute("ALTER TABLE usage ADD COLUMN web_searches INTEGER NOT NULL DEFAULT 0", []);
        // Lembretes que se repetem: none, daily, weekdays, weekly, monthly.
        let _ = conn.execute("ALTER TABLE reminders ADD COLUMN repeat TEXT NOT NULL DEFAULT 'none'", []);
        Ok(Self { conn })
    }

    /// O banco está íntegro? (verificação rápida do SQLite)
    pub fn healthy(&self) -> bool {
        self.conn
            .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .is_ok_and(|r| r == "ok")
    }

    /// Todas as linhas da tabela de configurações (para salvar ao consertar o banco).
    fn settings_rows(&self) -> Vec<(String, String)> {
        self.conn
            .prepare("SELECT key, value FROM settings")
            .and_then(|mut st| st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|rows| rows.flatten().collect()))
            .unwrap_or_default()
    }

    /// Abre o banco; se estiver corrompido, guarda o estragado numa pasta à parte, restaura a
    /// cópia de segurança íntegra mais nova e regrava por cima as configurações atuais (que
    /// costumam continuar legíveis). Devolve o banco e, se houve conserto, o que foi feito.
    pub fn open_or_repair(dir: &Path) -> Result<(Self, Option<String>), String> {
        let path = dir.join("pipo.db");
        let first = Self::open(&path);
        let rows = match &first {
            Ok(db) if db.healthy() => return Ok((first.unwrap(), None)),
            Ok(db) => db.settings_rows(),
            Err(_) => Vec::new(),
        };
        drop(first);

        let aside = dir.join(format!("corrompido-{}", chrono::Local::now().format("%Y%m%d-%H%M%S")));
        std::fs::create_dir_all(&aside).map_err(|e| e.to_string())?;
        for name in ["pipo.db", "pipo.db-wal", "pipo.db-shm"] {
            let file = dir.join(name);
            if file.exists() {
                std::fs::rename(&file, aside.join(name)).map_err(|e| format!("não consegui guardar {name}: {e}"))?;
            }
        }
        let backup = newest_healthy_backup(&dir.join("backups"));
        if let Some(b) = &backup {
            std::fs::copy(b, &path).map_err(|e| e.to_string())?;
        }
        let db = Self::open(&path).map_err(|e| e.to_string())?;
        for (key, value) in &rows {
            let _ = db.set_setting(key, value);
        }
        let from = backup.map(|b| b.display().to_string()).unwrap_or_else(|| "nenhuma cópia (banco novo)".into());
        Ok((db, Some(format!("banco corrompido guardado em {}; restaurado de {from}; {} configurações mantidas", aside.display(), rows.len()))))
    }

    /// Passa tudo o que está no diário (WAL) para o arquivo principal do banco.
    pub fn checkpoint(&self) {
        let _ = self.conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    }

    /// Resumo do que há no banco (para o registro de diagnóstico).
    pub fn stats(&self) -> String {
        let count = |table: &str| -> i64 {
            self.conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap_or(-1)
        };
        format!(
            "configurações={} conversas={} mensagens={} gastos={} xp={} notas={} lembretes={}",
            count("settings"),
            count("conversations"),
            count("messages"),
            count("usage"),
            count("xp_log"),
            count("notes"),
            count("reminders")
        )
    }

    /// Cópia de segurança consistente do banco (VACUUM INTO), mantendo as `keep` mais novas.
    pub fn backup(&self, dir: &Path, keep: usize) -> Result<std::path::PathBuf, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let name = format!("pipo-{}.db", chrono::Local::now().format("%Y%m%d-%H%M%S"));
        let target = dir.join(name);
        self.conn
            .execute("VACUUM INTO ?1", params![target.to_string_lossy()])
            .map_err(|e| e.to_string())?;
        let mut old: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("pipo-"))
            .map(|e| e.path())
            .collect();
        old.sort();
        while old.len() > keep {
            let _ = std::fs::remove_file(old.remove(0));
        }
        Ok(target)
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

/// Melhor cópia de segurança: entre as que abrem e passam na verificação de integridade, a
/// que tem mais dados (conversas, mensagens, gastos, XP, notas, lembretes); a mais nova desempata.
/// Só "a mais nova" não basta: uma cópia feita depois de um problema pode ter um estado antigo.
fn newest_healthy_backup(dir: &Path) -> Option<std::path::PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("pipo-")))
        .filter(|p| p.metadata().is_ok_and(|m| m.len() > 0))
        .collect();
    files.sort();
    let score = |p: &std::path::PathBuf| -> Option<i64> {
        let c = Connection::open_with_flags(p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
        let ok: String = c.query_row("PRAGMA quick_check", [], |r| r.get(0)).ok()?;
        if ok != "ok" {
            return None;
        }
        let count = |t: &str| c.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get::<_, i64>(0)).unwrap_or(0);
        Some(["conversations", "messages", "usage", "xp_log", "notes", "reminders"].iter().map(|t| count(t)).sum())
    };
    // `files` está em ordem cronológica; com `max_by_key`, no empate fica a mais nova (a última).
    files.into_iter().filter_map(|p| score(&p).map(|s| (s, p))).max_by_key(|(s, _)| *s).map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::Db;

    #[test]
    fn conserta_banco_corrompido_com_a_copia_boa() {
        let dir = std::env::temp_dir().join(format!("pipo-test-db-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("backups")).unwrap();
        // Cópia boa com uma conversa e uma configuração antiga.
        {
            let good = Db::open(&dir.join("backups").join("pipo-20260101-000000.db")).unwrap();
            good.conn().execute("INSERT INTO conversations DEFAULT VALUES", []).unwrap();
            good.set_setting("settings", "antiga").unwrap();
        }
        // Banco atual com configuração nova e páginas estragadas no meio do arquivo.
        {
            let db = Db::open(&dir.join("pipo.db")).unwrap();
            db.set_setting("settings", "nova").unwrap();
            for i in 0..300 {
                db.conn().execute("INSERT INTO notes (text) VALUES (?1)", [format!("nota {i} {}", "x".repeat(200))]).unwrap();
            }
        }
        let mut bytes = std::fs::read(dir.join("pipo.db")).unwrap();
        for b in bytes.iter_mut().skip(4096 * 3).take(4096 * 4) {
            *b = 0xAB;
        }
        std::fs::write(dir.join("pipo.db"), bytes).unwrap();

        let (db, repaired) = Db::open_or_repair(&dir).unwrap();
        assert!(repaired.is_some(), "deveria ter consertado");
        assert!(db.healthy());
        let conversations: i64 = db.conn().query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0)).unwrap();
        assert_eq!(conversations, 1, "veio da cópia boa");
        assert_eq!(db.get_setting("settings").as_deref(), Some("nova"), "configuração atual mantida");
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod real_copy {
    #[test]
    #[ignore = "usa uma copia do banco real na pasta temporaria pipo-real-test"]
    fn conserta_a_copia_do_banco_real() {
        let dir = std::env::temp_dir().join("pipo-real-test");
        let (db, repaired) = super::Db::open_or_repair(&dir).unwrap();
        println!("conserto: {repaired:?}");
        println!("depois: {} íntegro={}", db.stats(), db.healthy());
    }
}

//! Fase 9.4: diário automático. O Pipo anota sozinho o que aconteceu no PC — em que app
//! (e projeto) você passou cada trecho do dia, os avisos, downloads, hábitos cumpridos —
//! e junta com o que já está guardado (conversas, notas, partidas). Tudo fica só no
//! computador, por 90 dias, e dá para pesquisar ("quando baixei o contrato?").

use std::sync::Mutex;
use std::time::{Instant, SystemTime};

use chrono::{DateTime, Local, NaiveDate, NaiveTime};
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::context::{ActiveApp, Category};
use crate::documents::fold;
use crate::AppState;

/// Parado há mais que isso: fora do PC (o trecho termina).
const AWAY: u64 = 5 * 60;
/// Trechos mais curtos que isso não entram (uma olhada rápida no Discord não vira linha).
const MIN_BLOCK: i64 = 2 * 60;
/// Voltar ao mesmo app em até 5 min continua o mesmo trecho.
const BRIDGE: i64 = 5 * 60;
const KEEP_DAYS: i64 = 90;
const MAX_TITLES: usize = 8;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub day: String,
    pub start: String,
    /// Fim do trecho ("" para acontecimentos de um momento só).
    pub end: String,
    /// app, aviso, download, habito, chat, nota, lol
    pub kind: String,
    pub title: String,
    pub detail: String,
    /// Categoria do app ou tipo do aviso (decide o ícone).
    pub tag: String,
}

struct Block {
    key: String,
    title: String,
    category: &'static str,
    titles: Vec<String>,
    started: DateTime<Local>,
    seen: DateTime<Local>,
    row: Option<i64>,
    saved: Instant,
}

struct Watch {
    block: Option<Block>,
    downloads_since: Option<SystemTime>,
    last_scan: Option<Instant>,
    pruned: Option<NaiveDate>,
}

static WATCH: Mutex<Watch> = Mutex::new(Watch { block: None, downloads_since: None, last_scan: None, pruned: None });

pub fn create_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS journal (
             id     INTEGER PRIMARY KEY AUTOINCREMENT,
             day    TEXT NOT NULL,
             start  TEXT NOT NULL,
             end    TEXT NOT NULL DEFAULT '',
             kind   TEXT NOT NULL,
             title  TEXT NOT NULL,
             detail TEXT NOT NULL DEFAULT '',
             tag    TEXT NOT NULL DEFAULT ''
         );
         CREATE INDEX IF NOT EXISTS journal_day ON journal (day);",
    )
}

fn enabled(app: &AppHandle) -> (bool, bool) {
    let state = app.state::<AppState>();
    let s = state.settings.lock().unwrap();
    (s.journal, s.journal_titles)
}

fn category_name(c: Category) -> &'static str {
    match c {
        Category::Dev => "dev",
        Category::Docs => "docs",
        Category::Browser => "browser",
        Category::Live => "live",
        Category::Games => "games",
        Category::Chat => "chat",
        Category::Ai => "ai",
        Category::Other => "other",
    }
}

/// Título da janela sem o nome do app no fim; nada de janelas anônimas nem de cofres de senha.
fn clean_title(app: &ActiveApp) -> Option<String> {
    let exe = app.exe.to_lowercase();
    if ["keepass", "bitwarden", "1password", "lastpass", "dashlane"].iter().any(|p| exe.contains(p)) {
        return None;
    }
    let folded = fold(&app.title);
    if ["inprivate", "incognito", "anonima", "navegacao privada", "private browsing"].iter().any(|p| folded.contains(p)) {
        return None;
    }
    let mut title = app.title.trim().to_string();
    let app_words: Vec<String> = fold(&app.name).split_whitespace().filter(|w| w.len() > 2).map(String::from).collect();
    for sep in [" - ", " — ", " | "] {
        if let Some((rest, last)) = title.rsplit_once(sep) {
            let last = fold(last);
            let known = ["google chrome", "microsoft edge", "mozilla firefox", "opera", "brave", "visual studio code", "cursor"];
            if known.iter().any(|k| last.contains(k)) || app_words.iter().any(|w| last.contains(w.as_str())) {
                title = rest.trim().to_string();
            }
        }
    }
    // "● arquivo.rs" (não salvo) e contadores "(3) WhatsApp".
    let title = title.trim_start_matches(['●', '•', '*', ' ']).to_string();
    let title = match title.strip_prefix('(').and_then(|r| r.split_once(") ")) {
        Some((n, rest)) if n.chars().all(|c| c.is_ascii_digit()) => rest.to_string(),
        _ => title,
    };
    let title: String = title.chars().take(80).collect();
    if title.is_empty() || fold(&title) == fold(&app.name) {
        return None;
    }
    Some(title)
}

fn hm(t: &DateTime<Local>) -> String {
    t.format("%H:%M").to_string()
}

fn minutes_of(hm: &str) -> Option<i64> {
    NaiveTime::parse_from_str(hm, "%H:%M").ok().map(|t| chrono::Timelike::num_seconds_from_midnight(&t) as i64)
}

/// Grava (ou atualiza) o trecho, emendando no anterior do mesmo app se a pausa foi curta.
fn save(app: &AppHandle, b: &mut Block) {
    if b.row.is_none() && (b.seen - b.started).num_seconds() < MIN_BLOCK {
        return;
    }
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let conn = db.conn();
    let detail = b.titles.join(" · ");
    let (day, start, end) = (b.started.format("%Y-%m-%d").to_string(), hm(&b.started), hm(&b.seen));
    if let Some(id) = b.row {
        let _ = conn.execute("UPDATE journal SET end = ?1, detail = ?2 WHERE id = ?3", params![end, detail, id]);
        return;
    }
    let previous: Option<(i64, String, String)> = conn
        .query_row(
            "SELECT id, end, detail FROM journal WHERE day = ?1 AND kind = 'app' ORDER BY id DESC LIMIT 1",
            params![day],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    if let Some((id, previous_end, previous_detail)) = previous {
        let same: bool = conn.query_row("SELECT title = ?1 FROM journal WHERE id = ?2", params![b.title, id], |r| r.get(0)).unwrap_or(false);
        let gap = minutes_of(&start).zip(minutes_of(&previous_end)).map(|(s, e)| s - e);
        if same && gap.is_some_and(|g| (0..=BRIDGE).contains(&g)) {
            let mut titles: Vec<String> = previous_detail.split(" · ").filter(|t| !t.is_empty()).map(String::from).collect();
            for t in &b.titles {
                if !titles.contains(t) && titles.len() < MAX_TITLES {
                    titles.push(t.clone());
                }
            }
            b.titles = titles;
            let _ = conn.execute("UPDATE journal SET end = ?1, detail = ?2 WHERE id = ?3", params![end, b.titles.join(" · "), id]);
            b.row = Some(id);
            return;
        }
    }
    if conn
        .execute(
            "INSERT INTO journal (day, start, end, kind, title, detail, tag) VALUES (?1, ?2, ?3, 'app', ?4, ?5, ?6)",
            params![day, start, end, b.title, detail, b.category],
        )
        .is_ok()
    {
        b.row = Some(conn.last_insert_rowid());
    }
}

/// Chamado pelos sensores a cada 2 s com o app em uso.
pub fn observe(app: &AppHandle, current: Option<&ActiveApp>, idle: u64) {
    let (on, keep_titles) = enabled(app);
    let now = Local::now();
    let active = current.filter(|_| on && idle < AWAY);
    let key = active.map(|c| format!("{}|{}", c.name, c.project.clone().unwrap_or_default()));
    let mut changed = false;
    {
        let mut w = WATCH.lock().unwrap();
        let ended = match (&w.block, &key) {
            (Some(b), Some(k)) => &b.key != k || b.started.date_naive() != now.date_naive(),
            (Some(_), None) => true,
            _ => false,
        };
        if ended {
            if let Some(mut b) = w.block.take() {
                save(app, &mut b);
                changed |= b.row.is_some();
            }
        }
        if let (Some(c), Some(key)) = (active, key) {
            let block = w.block.get_or_insert_with(|| Block {
                key,
                title: match &c.project {
                    Some(p) => format!("{} · {p}", c.name),
                    None => c.name.clone(),
                },
                category: category_name(c.category),
                titles: Vec::new(),
                started: now,
                seen: now,
                row: None,
                saved: Instant::now(),
            });
            block.seen = now;
            if keep_titles {
                if let Some(t) = clean_title(c) {
                    if !block.titles.contains(&t) && block.titles.len() < MAX_TITLES {
                        block.titles.push(t);
                    }
                }
            }
            if block.saved.elapsed().as_secs() >= 60 {
                block.saved = Instant::now();
                let was = block.row.is_some();
                save(app, block);
                changed |= !was && block.row.is_some();
            }
        }
    }
    if on {
        scan_downloads(app);
    }
    prune(app);
    if changed {
        let _ = app.emit("journal-changed", ());
    }
}

/// Arquivos novos na pasta Downloads (a cada minuto).
fn scan_downloads(app: &AppHandle) {
    let since = {
        let mut w = WATCH.lock().unwrap();
        if w.last_scan.is_some_and(|t| t.elapsed().as_secs() < 60) {
            return;
        }
        w.last_scan = Some(Instant::now());
        match w.downloads_since {
            Some(t) => t,
            None => {
                // Primeira passada: só vale o que chegar daqui para frente.
                w.downloads_since = Some(SystemTime::now());
                return;
            }
        }
    };
    let found = crate::actions::downloads::finished_since(since);
    if let Some(latest) = found.iter().map(|f| f.2).max() {
        WATCH.lock().unwrap().downloads_since = Some(latest);
    }
    for (name, bytes, modified) in found {
        let at: DateTime<Local> = modified.into();
        insert(app, &at, "download", &name, &size_text(bytes), "");
    }
}

fn prune(app: &AppHandle) {
    let today = Local::now().date_naive();
    {
        let mut w = WATCH.lock().unwrap();
        if w.pruned == Some(today) {
            return;
        }
        w.pruned = Some(today);
    }
    let cutoff = (today - chrono::Duration::days(KEEP_DAYS)).format("%Y-%m-%d").to_string();
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let _ = db.conn().execute("DELETE FROM journal WHERE day < ?1", [cutoff]);
}

fn size_text(bytes: u64) -> String {
    if bytes < 1024 * 1024 {
        format!("{} KB", bytes.div_ceil(1024))
    } else {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0).replace('.', ",")
    }
}

fn insert(app: &AppHandle, at: &DateTime<Local>, kind: &str, title: &str, detail: &str, tag: &str) {
    {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        let _ = db.conn().execute(
            "INSERT INTO journal (day, start, kind, title, detail, tag) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![at.format("%Y-%m-%d").to_string(), hm(at), kind, title, detail, tag],
        );
    }
    let _ = app.emit("journal-changed", ());
}

/// Anota um acontecimento agora (aviso, hábito cumprido…), se o diário estiver ligado.
pub fn add(app: &AppHandle, kind: &str, title: &str, detail: &str, tag: &str) {
    if !enabled(app).0 || title.trim().is_empty() {
        return;
    }
    insert(app, &Local::now(), kind, title, detail, tag);
}

/// Texto que a pessoa escreveu numa mensagem (o conteúdo é a lista de blocos da API).
fn user_text(content: &str) -> Option<String> {
    let value: Value = serde_json::from_str(content).ok()?;
    let text = match &value {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks.iter().rev().find(|b| b["type"] == "text")?["text"].as_str()?.to_string(),
        _ => return None,
    };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

fn short(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    format!("{}…", text.chars().take(max - 1).collect::<String>().trim_end())
}

fn stored(conn: &Connection, filter: &str, arg: &str) -> Vec<Entry> {
    let sql = format!("SELECT day, start, end, kind, title, detail, tag FROM journal WHERE {filter} ORDER BY day, start, id");
    conn.prepare(&sql)
        .and_then(|mut st| {
            st.query_map([arg], |r| {
                Ok(Entry { day: r.get(0)?, start: r.get(1)?, end: r.get(2)?, kind: r.get(3)?, title: r.get(4)?, detail: r.get(5)?, tag: r.get(6)? })
            })
            .map(|rows| rows.flatten().collect())
        })
        .unwrap_or_default()
}

/// Conversas com o Pipo (pelo chat ou Telegram) desde `since` (dia, inclusive) até `until` (exclusive).
fn chats(conn: &Connection, since: &str, until: &str) -> Vec<Entry> {
    let mut st = match conn.prepare(
        "SELECT conversation_id, datetime(created_at, 'localtime') t, content FROM messages
         WHERE role = 'user' AND date(created_at, 'localtime') >= ?1 AND date(created_at, 'localtime') < ?2
         ORDER BY id",
    ) {
        Ok(st) => st,
        Err(_) => return Vec::new(),
    };
    let rows: Vec<(i64, String, String)> =
        st.query_map([since, until], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|r| r.flatten().collect()).unwrap_or_default();
    // Uma linha por conversa e dia: a primeira pergunta e quantas vieram depois.
    let mut out: Vec<(i64, Entry, usize)> = Vec::new();
    for (conversation, at, content) in rows {
        let Some(text) = user_text(&content) else { continue };
        let (day, time) = at.split_once(' ').unwrap_or((&at, ""));
        if let Some(entry) = out.iter_mut().find(|(c, e, _)| *c == conversation && e.day == day) {
            entry.2 += 1;
            entry.1.end = time.get(..5).unwrap_or("").to_string();
            continue;
        }
        let entry = Entry {
            day: day.to_string(),
            start: time.get(..5).unwrap_or("").to_string(),
            end: String::new(),
            kind: "chat".into(),
            title: short(&text, 90),
            detail: String::new(),
            tag: String::new(),
        };
        out.push((conversation, entry, 1));
    }
    out.into_iter()
        .map(|(_, mut e, n)| {
            if n > 1 {
                e.detail = format!("{n} perguntas");
            } else {
                e.end.clear();
            }
            e
        })
        .collect()
}

fn notes(conn: &Connection, since: &str, until: &str) -> Vec<Entry> {
    conn.prepare(
        "SELECT datetime(created_at, 'localtime'), text FROM notes
         WHERE date(created_at, 'localtime') >= ?1 AND date(created_at, 'localtime') < ?2 ORDER BY id",
    )
    .and_then(|mut st| {
        st.query_map([since, until], |r| {
            let at: String = r.get(0)?;
            let text: String = r.get(1)?;
            let (day, time) = at.split_once(' ').map(|(d, t)| (d.to_string(), t.get(..5).unwrap_or("").to_string())).unwrap_or_default();
            Ok(Entry { day, start: time, end: String::new(), kind: "nota".into(), title: short(&text, 90), detail: String::new(), tag: String::new() })
        })
        .map(|rows| rows.flatten().collect())
    })
    .unwrap_or_default()
}

fn games(conn: &Connection, since: &str, until: &str) -> Vec<Entry> {
    conn.prepare(
        "SELECT day, COALESCE(time(played_at / 1000, 'unixepoch', 'localtime'), time(created_at, 'localtime')),
                champion, won, kills, deaths, assists, minutes FROM lol_games
         WHERE day >= ?1 AND day < ?2 ORDER BY COALESCE(played_at / 1000, strftime('%s', created_at))",
    )
    .and_then(|mut st| {
        st.query_map([since, until], |r| {
            let won: bool = r.get(3)?;
            let (k, d, a): (i64, i64, i64) = (r.get(4)?, r.get(5)?, r.get(6)?);
            let minutes: f64 = r.get(7)?;
            let time: String = r.get(1)?;
            Ok(Entry {
                day: r.get(0)?,
                start: time.get(..5).unwrap_or("").to_string(),
                end: String::new(),
                kind: "lol".into(),
                title: format!("{} de {}", if won { "Vitória" } else { "Derrota" }, r.get::<_, String>(2)?),
                detail: format!("{k}/{d}/{a} · {minutes:.0} min"),
                tag: if won { "win".into() } else { "loss".into() },
            })
        })
        .map(|rows| rows.flatten().collect())
    })
    .unwrap_or_default()
}

fn next_day(day: &str) -> String {
    NaiveDate::parse_from_str(day, "%Y-%m-%d").map(|d| (d + chrono::Duration::days(1)).format("%Y-%m-%d").to_string()).unwrap_or_default()
}

/// Tudo de um dia, em ordem de horário.
pub fn day(app: &AppHandle, day: &str) -> Vec<Entry> {
    // O trecho em andamento entra com o horário de agora.
    {
        let mut w = WATCH.lock().unwrap();
        if let Some(b) = w.block.as_mut() {
            save(app, b);
        }
    }
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let conn = db.conn();
    let until = next_day(day);
    let mut all = stored(conn, "day = ?1", day);
    all.extend(chats(conn, day, &until));
    all.extend(notes(conn, day, &until));
    all.extend(games(conn, day, &until));
    all.sort_by(|a, b| (&a.day, &a.start).cmp(&(&b.day, &b.start)));
    all
}

/// Todas as palavras da busca aparecem (sem ligar para acento e maiúsculas).
fn matches(entry: &Entry, words: &[String]) -> bool {
    let text = fold(&format!("{} {} {}", entry.title, entry.detail, entry.kind));
    words.iter().all(|w| text.contains(w.as_str()))
}

/// Procura nos últimos 90 dias, do mais recente para o mais antigo.
pub fn search(app: &AppHandle, query: &str, limit: usize) -> Vec<Entry> {
    let words: Vec<String> = fold(query).split_whitespace().map(String::from).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let since = (Local::now().date_naive() - chrono::Duration::days(KEEP_DAYS)).format("%Y-%m-%d").to_string();
    let until = next_day(&Local::now().format("%Y-%m-%d").to_string());
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let conn = db.conn();
    let mut all = stored(conn, "day >= ?1", &since);
    all.extend(chats(conn, &since, &until));
    all.extend(notes(conn, &since, &until));
    all.extend(games(conn, &since, &until));
    let mut found: Vec<Entry> = all.into_iter().filter(|e| matches(e, &words)).collect();
    found.sort_by(|a, b| (&b.day, &b.start).cmp(&(&a.day, &a.start)));
    found.truncate(limit);
    found
}

/// Uma linha de texto (para o chat e o Telegram).
pub fn line(e: &Entry, with_day: bool) -> String {
    let when = if e.end.is_empty() || e.end == e.start { e.start.clone() } else { format!("{}–{}", e.start, e.end) };
    let day = if with_day { format!("{} ", day_label(&e.day)) } else { String::new() };
    let what = match e.kind.as_str() {
        "app" => e.title.clone(),
        "aviso" => format!("Aviso: {}", e.title),
        "download" => format!("Baixou {}", e.title),
        "habito" => format!("Hábito cumprido: {}", e.title),
        "chat" => format!("Perguntou ao Pipo: {}", e.title),
        "nota" => format!("Nota: {}", e.title),
        _ => e.title.clone(),
    };
    let detail = if e.detail.is_empty() { String::new() } else { format!(" ({})", short(&e.detail, 140)) };
    format!("{day}{when} {what}{detail}")
}

fn day_label(day: &str) -> String {
    NaiveDate::parse_from_str(day, "%Y-%m-%d").map(|d| d.format("%d/%m").to_string()).unwrap_or_else(|_| day.to_string())
}

/// "hoje", "ontem", "anteontem", "AAAA-MM-DD" ou "DD/MM".
pub fn parse_day(text: &str) -> Option<String> {
    let today = Local::now().date_naive();
    let t = fold(text.trim());
    let date = match t.as_str() {
        "" | "hoje" => today,
        "ontem" => today - chrono::Duration::days(1),
        "anteontem" => today - chrono::Duration::days(2),
        other => NaiveDate::parse_from_str(other, "%Y-%m-%d")
            .ok()
            .or_else(|| NaiveDate::parse_from_str(&format!("{other}/{}", today.format("%Y")), "%d/%m/%Y").ok())?,
    };
    Some(date.format("%Y-%m-%d").to_string())
}

/// Resumo de um dia em texto, para a Claude (sem os detalhes repetidos de cada janela).
pub fn day_text(app: &AppHandle, day: &str) -> String {
    let entries = self::day(app, day);
    if entries.is_empty() {
        return format!("Nada anotado no diário em {}.", day_label(day));
    }
    let mut out = vec![format!("Diário de {}:", day_label(day))];
    out.extend(entries.iter().map(|e| line(e, false)));
    out.join("\n")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalDay {
    day: String,
    entries: Vec<Entry>,
    /// Há algo anotado antes deste dia? (para a seta "dia anterior")
    has_before: bool,
}

#[tauri::command]
pub fn journal_day(app: AppHandle, day: Option<String>) -> JournalDay {
    let day = day.and_then(|d| parse_day(&d)).unwrap_or_else(|| Local::now().format("%Y-%m-%d").to_string());
    let entries = self::day(&app, &day);
    let has_before = {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        let conn = db.conn();
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM journal WHERE day < ?1) OR EXISTS(SELECT 1 FROM messages WHERE date(created_at, 'localtime') < ?1)",
            [&day],
            |r| r.get(0),
        )
        .unwrap_or(false)
    };
    JournalDay { day, entries, has_before }
}

#[tauri::command]
pub fn journal_search(app: AppHandle, query: String) -> Vec<Entry> {
    search(&app, &query, 80)
}

/// Apaga o que o diário anotou (conversas e notas continuam nos lugares delas).
#[tauri::command]
pub fn journal_clear(app: AppHandle) -> Result<(), String> {
    WATCH.lock().unwrap().block = None;
    {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        db.conn().execute("DELETE FROM journal", []).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("journal-changed", ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active(name: &str, exe: &str, title: &str) -> ActiveApp {
        ActiveApp {
            exe: exe.into(),
            name: name.into(),
            title: title.into(),
            category: Category::Other,
            project: None,
            fullscreen: false,
            monitor: (0, 0, 0, 0),
        }
    }

    #[test]
    fn limpa_o_titulo_da_janela() {
        assert_eq!(clean_title(&active("Chrome", "chrome.exe", "Contrato aluguel.pdf - Google Chrome")).as_deref(), Some("Contrato aluguel.pdf"));
        assert_eq!(clean_title(&active("VS Code", "Code.exe", "● journal.rs - pipo - Visual Studio Code")).as_deref(), Some("journal.rs - pipo"));
        assert_eq!(clean_title(&active("WhatsApp", "WhatsApp.exe", "(3) WhatsApp")), None);
        assert_eq!(clean_title(&active("Edge", "msedge.exe", "Banco - [InPrivate] - Microsoft Edge")), None);
        assert_eq!(clean_title(&active("KeePass", "KeePass.exe", "Database.kdbx")), None);
        assert_eq!(clean_title(&active("Spotify", "Spotify.exe", "Spotify")), None);
    }

    #[test]
    fn entende_o_dia_pedido() {
        let today = Local::now().date_naive();
        assert_eq!(parse_day("hoje"), Some(today.format("%Y-%m-%d").to_string()));
        assert_eq!(parse_day("Ontem"), Some((today - chrono::Duration::days(1)).format("%Y-%m-%d").to_string()));
        assert_eq!(parse_day("2026-11-03"), Some("2026-11-03".into()));
        assert_eq!(parse_day("03/11"), Some(format!("{}-11-03", today.format("%Y"))));
        assert_eq!(parse_day("sei la"), None);
    }

    #[test]
    fn junta_as_perguntas_da_mesma_conversa() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE messages (id INTEGER PRIMARY KEY, conversation_id INTEGER, role TEXT, content TEXT, created_at TEXT);
             INSERT INTO messages VALUES
               (1, 7, 'user', '[{\"type\":\"text\",\"text\":\"como faço  um   bolo?\"}]', '2026-11-04 12:00:00'),
               (2, 7, 'assistant', '[{\"type\":\"text\",\"text\":\"assim\"}]', '2026-11-04 12:00:05'),
               (3, 7, 'user', '[{\"type\":\"tool_result\",\"content\":\"x\"}]', '2026-11-04 12:00:06'),
               (4, 7, 'user', '[{\"type\":\"text\",\"text\":\"e de chocolate?\"}]', '2026-11-04 12:10:00'),
               (5, 8, 'user', '[{\"type\":\"document\"},{\"type\":\"text\",\"text\":\"resume o anexo\"}]', '2026-11-04 15:00:00');",
        )
        .unwrap();
        let list = chats(&conn, "2026-11-01", "2026-11-09");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].title, "como faço um bolo?");
        assert_eq!(list[0].detail, "2 perguntas");
        assert_eq!(list[1].title, "resume o anexo");
        let words = vec!["bolo".to_string()];
        assert!(matches(&list[0], &words) && !matches(&list[1], &words));
    }

    #[test]
    fn escreve_a_linha_do_diario() {
        let e = Entry {
            day: "2026-11-04".into(),
            start: "09:10".into(),
            end: "10:40".into(),
            kind: "app".into(),
            title: "VS Code · pipo".into(),
            detail: "journal.rs".into(),
            tag: "dev".into(),
        };
        assert_eq!(line(&e, true), "04/11 09:10–10:40 VS Code · pipo (journal.rs)");
        let d = Entry { kind: "download".into(), title: "contrato.pdf".into(), end: String::new(), detail: "2,1 MB".into(), ..e };
        assert_eq!(line(&d, false), "09:10 Baixou contrato.pdf (2,1 MB)");
    }
}

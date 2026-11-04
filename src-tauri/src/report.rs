//! Fase 8.0: relatório semanal. Junta o que o Pipo já registra no PC (tempo por tipo de
//! app, pomodoros, conversas, XP, conquistas, partidas de LoL, gastos com a IA) e os
//! commits da semana no GitHub, com um comentário curto da Claude. Sai no dia e hora
//! escolhidos, na aba Hoje e no Telegram.

use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{Datelike, Duration, Local, NaiveDate};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

const KEY: &str = "weekly_report";
static RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Semana do relatório ("2026-W45"), para não repetir.
    pub week: String,
    pub text: String,
    pub created_at: String,
}

/// Números de um período [from, to) em dias locais "AAAA-MM-DD".
#[derive(Default, Debug, PartialEq)]
pub struct Stats {
    pub active_secs: i64,
    /// (categoria, segundos), da maior para a menor.
    pub categories: Vec<(String, i64)>,
    pub top_apps: Vec<(String, i64)>,
    pub pomodoros: i64,
    pub chats: i64,
    pub actions: i64,
    pub xp: i64,
    pub achievements: i64,
    pub cost: f64,
    pub lol_games: i64,
    pub lol_wins: i64,
}

fn sum(conn: &Connection, sql: &str, from: &str, to: &str) -> i64 {
    conn.query_row(sql, params![from, to], |r| r.get::<_, Option<i64>>(0)).ok().flatten().unwrap_or(0)
}

pub fn stats(conn: &Connection, from: NaiveDate, to: NaiveDate) -> Stats {
    let (f, t) = (from.format("%Y-%m-%d").to_string(), to.format("%Y-%m-%d").to_string());
    let pairs = |sql: &str| -> Vec<(String, i64)> {
        conn.prepare(sql)
            .and_then(|mut st| st.query_map(params![f, t], |r| Ok((r.get(0)?, r.get(1)?))).map(|rows| rows.flatten().collect()))
            .unwrap_or_default()
    };
    let xp_times = |kind: &str| {
        conn.query_row("SELECT COALESCE(SUM(times), 0) FROM xp_log WHERE day >= ?1 AND day < ?2 AND kind = ?3", params![f, t, kind], |r| r.get(0))
            .unwrap_or(0)
    };
    // Gastos e conquistas usam o horário UTC do banco: o recorte por dia local fica aproximado.
    let cost: f64 = conn
        .query_row("SELECT COALESCE(SUM(cost), 0) FROM usage WHERE date(created_at, 'localtime') >= ?1 AND date(created_at, 'localtime') < ?2", params![f, t], |r| r.get(0))
        .unwrap_or(0.0);
    Stats {
        active_secs: sum(conn, "SELECT SUM(seconds) FROM activity WHERE day >= ?1 AND day < ?2", &f, &t),
        categories: pairs("SELECT category, SUM(seconds) s FROM activity WHERE day >= ?1 AND day < ?2 GROUP BY category ORDER BY s DESC"),
        top_apps: pairs("SELECT app, SUM(seconds) s FROM activity WHERE day >= ?1 AND day < ?2 GROUP BY app ORDER BY s DESC LIMIT 3"),
        pomodoros: xp_times("pomodoro"),
        chats: xp_times("chat"),
        actions: xp_times("action"),
        xp: sum(conn, "SELECT SUM(xp) FROM xp_log WHERE day >= ?1 AND day < ?2", &f, &t),
        achievements: sum(conn, "SELECT COUNT(*) FROM achievements WHERE date(unlocked_at, 'localtime') >= ?1 AND date(unlocked_at, 'localtime') < ?2", &f, &t),
        cost,
        lol_games: sum(conn, "SELECT COUNT(*) FROM lol_games WHERE day >= ?1 AND day < ?2", &f, &t),
        lol_wins: sum(conn, "SELECT COUNT(*) FROM lol_games WHERE day >= ?1 AND day < ?2 AND won = 1", &f, &t),
    }
}

fn hours(secs: i64) -> String {
    let (h, m) = (secs / 3600, secs % 3600 / 60);
    match (h, m) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h{m:02}"),
    }
}

/// "(+3h vs. semana passada)"
fn versus(now: i64, before: i64) -> String {
    let diff = now - before;
    if before == 0 || diff.abs() < 1800 {
        return String::new();
    }
    format!(" ({}{} vs. semana passada)", if diff > 0 { "+" } else { "−" }, hours(diff.abs()))
}

/// Texto do relatório (sem o comentário da IA).
pub fn render(week: &Stats, previous: &Stats, from: NaiveDate, to: NaiveDate, commits: Option<(i64, usize)>, level: i64) -> String {
    let last = to - Duration::days(1);
    let mut lines = vec![format!("📊 Sua semana no PC ({} a {})", from.format("%d/%m"), last.format("%d/%m"))];
    if week.active_secs == 0 {
        lines.push("O Pipo não registrou uso do PC nesta semana.".into());
    } else {
        lines.push(format!("⏱ {} no PC{}", hours(week.active_secs), versus(week.active_secs, previous.active_secs)));
        let parts: Vec<String> = week
            .categories
            .iter()
            .filter(|(_, s)| *s >= 600)
            .take(4)
            .map(|(c, s)| format!("{} {}", crate::proactive::label(c).to_lowercase(), hours(*s)))
            .collect();
        if !parts.is_empty() {
            lines.push(format!("   {}", parts.join(" · ")));
        }
        if !week.top_apps.is_empty() {
            lines.push(format!("   Mais usados: {}", week.top_apps.iter().map(|(a, s)| format!("{a} ({})", hours(*s))).collect::<Vec<_>>().join(", ")));
        }
    }
    if week.pomodoros > 0 {
        lines.push(format!("🍅 {} pomodoro(s) concluído(s)", week.pomodoros));
    }
    if let Some((count, repos)) = commits.filter(|(c, _)| *c > 0) {
        lines.push(format!("💻 {count} commit(s) em {repos} repositório(s)"));
    }
    if week.lol_games > 0 {
        let rate = week.lol_wins as f64 * 100.0 / week.lol_games as f64;
        lines.push(format!("🎮 LoL: {} partida(s), {} vitória(s) ({rate:.0}%)", week.lol_games, week.lol_wins));
    }
    let mut life = vec![format!("+{} XP", week.xp), format!("nível {level}")];
    if week.achievements > 0 {
        life.push(format!("{} conquista(s)", week.achievements));
    }
    lines.push(format!("⭐ {}", life.join(" · ")));
    lines.push(format!(
        "💬 {} mensagem(ns) ao Pipo · {} ação(ões) · US$ {:.2} em IA",
        week.chats,
        week.actions,
        week.cost
    ));
    lines.join("\n")
}

/// Commits da semana em todas as contas do GitHub CLI: (quantos, em quantos repositórios).
fn commits_since(from: NaiveDate) -> Option<(i64, usize)> {
    let since = format!(">={}", from.format("%Y-%m-%d"));
    let mut total = 0i64;
    let mut repos = std::collections::HashSet::new();
    let accounts = crate::world::github::accounts();
    if accounts.is_empty() {
        return None;
    }
    for account in accounts {
        let Ok(out) = crate::world::github::gh_as(
            &account,
            &["search", "commits", "--author", &account, "--author-date", &since, "--limit", "200", "--json", "repository"],
        ) else {
            continue;
        };
        let Ok(list) = serde_json::from_str::<serde_json::Value>(&out) else { continue };
        for c in list.as_array().into_iter().flatten() {
            total += 1;
            if let Some(name) = c["repository"]["fullName"].as_str() {
                repos.insert(name.to_string());
            }
        }
    }
    Some((total, repos.len()))
}

fn week_key(day: NaiveDate) -> String {
    let w = day.iso_week();
    format!("{}-W{:02}", w.year(), w.week())
}

pub fn latest(app: &AppHandle) -> Option<Report> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.get_setting(KEY).and_then(|s| serde_json::from_str(&s).ok())
}

/// Monta o relatório dos últimos 7 dias (até ontem), guarda e devolve o texto.
pub async fn generate(app: &AppHandle) -> Result<String, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("O relatório já está sendo montado.".into());
    }
    let result = build(app).await;
    RUNNING.store(false, Ordering::SeqCst);
    result
}

async fn build(app: &AppHandle) -> Result<String, String> {
    let today = Local::now().date_naive();
    let (from, to) = (today - Duration::days(7), today);
    let (week, previous, level) = {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        let level = crate::life::level_now(db.conn());
        (stats(db.conn(), from, to), stats(db.conn(), from - Duration::days(7), from), level)
    };
    let commits = tauri::async_runtime::spawn_blocking(move || commits_since(from)).await.ok().flatten();
    let mut text = render(&week, &previous, from, to, commits, level);
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    if settings.weekly_report_ai && crate::secrets::get().is_some() && week.active_secs > 0 {
        let system = "Você é o Pipo, assistente de desktop com humor leve. Escreva em português do Brasil.";
        let prompt = format!(
            "Este é o resumo da semana do usuário no computador:\n\n{text}\n\nEscreva um comentário de no máximo 3 frases: \
             um destaque positivo, uma observação útil e uma sugestão concreta para a próxima semana. Sem repetir os números todos, sem títulos."
        );
        if let Ok(comment) = crate::ai::ask(app, system, &prompt, 300).await {
            text.push_str(&format!("\n\n🐾 {}", comment.trim()));
        }
    }
    let report = Report { week: week_key(today), text: text.clone(), created_at: Local::now().format("%d/%m %H:%M").to_string() };
    {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        db.set_setting(KEY, &serde_json::to_string(&report).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("report-changed", ());
    Ok(text)
}

/// Chamado pelos sensores a cada minuto: no dia e hora escolhidos, uma vez por semana.
pub fn maybe_run(app: &AppHandle) {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    if !settings.weekly_report {
        return;
    }
    let now = Local::now();
    if now.weekday().number_from_monday() != settings.weekly_report_day as u32 {
        return;
    }
    let Ok(at) = chrono::NaiveTime::parse_from_str(&settings.weekly_report_time, "%H:%M") else { return };
    if now.time() < at || latest(app).is_some_and(|r| r.week == week_key(now.date_naive())) || RUNNING.load(Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match generate(&app).await {
            Ok(text) => {
                crate::toast::notify(&app, "info", "Seu relatório da semana chegou", "Está na aba Hoje (e no Telegram, se ligado).", Some("view:today"));
                let _ = crate::world::telegram::send_text(&app, &text).await;
            }
            Err(e) => {
                if let Ok(dir) = app.path().app_data_dir() {
                    crate::log(&dir, &format!("relatório semanal falhou: {e}"));
                }
            }
        }
    });
}

#[tauri::command]
pub fn report_latest(app: AppHandle) -> Option<Report> {
    latest(&app)
}

#[tauri::command]
pub async fn report_generate(app: AppHandle) -> Result<Report, String> {
    generate(&app).await?;
    latest(&app).ok_or_else(|| "Não consegui salvar o relatório.".into())
}

#[tauri::command]
pub async fn report_send(app: AppHandle) -> Result<(), String> {
    let report = latest(&app).ok_or("Ainda não há relatório.")?;
    crate::world::telegram::send_text(&app, &report.text).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soma_a_semana_e_compara_com_a_anterior() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE activity (day TEXT, app TEXT, category TEXT, seconds INTEGER, PRIMARY KEY (day, app));
             CREATE TABLE xp_log (id INTEGER PRIMARY KEY, day TEXT, kind TEXT, times INTEGER, xp INTEGER, created_at TEXT);
             CREATE TABLE usage (id INTEGER PRIMARY KEY, cost REAL, created_at TEXT);
             CREATE TABLE achievements (id TEXT PRIMARY KEY, unlocked_at TEXT);
             CREATE TABLE lol_games (id INTEGER PRIMARY KEY, day TEXT, won INTEGER);
             INSERT INTO activity VALUES ('2026-11-02', 'VS Code', 'dev', 7200), ('2026-11-03', 'League of Legends', 'games', 3600),
                                         ('2026-10-27', 'VS Code', 'dev', 1800);
             INSERT INTO xp_log (day, kind, times, xp) VALUES ('2026-11-02', 'pomodoro', 3, 30), ('2026-11-03', 'chat', 5, 25);
             INSERT INTO lol_games (day, won) VALUES ('2026-11-03', 1), ('2026-11-03', 0);",
        )
        .unwrap();
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let week = stats(&conn, d("2026-10-28"), d("2026-11-04"));
        assert_eq!(week.active_secs, 10800);
        assert_eq!(week.pomodoros, 3);
        assert_eq!(week.chats, 5);
        assert_eq!(week.xp, 55);
        assert_eq!((week.lol_games, week.lol_wins), (2, 1));
        let before = stats(&conn, d("2026-10-21"), d("2026-10-28"));
        assert_eq!(before.active_secs, 1800);
        let text = render(&week, &before, d("2026-10-28"), d("2026-11-04"), Some((12, 3)), 7);
        assert!(text.contains("28/10 a 03/11"));
        assert!(text.contains("3h no PC (+2h30 vs. semana passada)"));
        assert!(text.contains("12 commit(s) em 3 repositório(s)"));
        assert!(text.contains("2 partida(s), 1 vitória(s) (50%)"));
        assert!(text.contains("nível 7"));
    }
}

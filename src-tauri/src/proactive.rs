//! Fase 6.3: Pipo proativo. Três partes, tudo guardado só no computador:
//! - Registro do dia: quanto tempo em cada tipo de app (para o resumo do fim do dia).
//! - Memória: fatos que o usuário pede para o Pipo lembrar (entram no chat).
//! - Iniciativa equilibrada: dicas pelo contexto, no máximo 3 por hora e com 12 min
//!   de intervalo, nunca em jogo, foco de pomodoro ou com o usuário longe do computador.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::actions::{focus, projects};
use crate::context::{ActiveApp, Category};
use crate::{clipboard, toast, AppState};

const MAX_PER_HOUR: usize = 3;
const MIN_GAP: Duration = Duration::from_secs(12 * 60);
/// Parado há mais que isso: o usuário não está no computador.
const AWAY: u64 = 5 * 60;

fn category_name(c: Category) -> &'static str {
    match c {
        Category::Dev => "dev",
        Category::Docs => "docs",
        Category::Browser => "browser",
        Category::Live => "live",
        Category::Games => "games",
        Category::Chat => "chat",
        Category::Other => "other",
    }
}

#[derive(Default)]
struct State {
    /// Segundos ainda não gravados: (app, categoria) -> segundos.
    pending: HashMap<(String, &'static str), i64>,
    last_flush: Option<Instant>,
    spoken: VecDeque<Instant>,
    /// Início da sessão contínua de uso (zera ao ficar 5 min parado).
    session_since: Option<Instant>,
    session_warned: bool,
    commit_nudged: HashMap<String, Instant>,
    last_commit_check: Option<Instant>,
    night_warned: Option<chrono::NaiveDate>,
    summary_done: Option<chrono::NaiveDate>,
    last_error_tip: Option<Instant>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap();
    f(guard.get_or_insert_with(State::default))
}

// ---------- Registro do dia ----------

/// Chamado pelos sensores a cada 2 s com o app em uso.
pub fn record(app: &AppHandle, current: Option<&ActiveApp>, idle: u64) {
    if idle < 120 {
        if let Some(c) = current {
            with(|s| *s.pending.entry((c.name.clone(), category_name(c.category))).or_default() += 2);
        }
    }
    let due = with(|s| {
        let due = s.last_flush.is_none_or(|t| t.elapsed() >= Duration::from_secs(60));
        if due {
            s.last_flush = Some(Instant::now());
        }
        due
    });
    if due {
        flush(app);
    }
}

fn flush(app: &AppHandle) {
    let pending = with(|s| std::mem::take(&mut s.pending));
    if pending.is_empty() {
        return;
    }
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    for ((name, category), secs) in pending {
        let _ = db.conn().execute(
            "INSERT INTO activity (day, app, category, seconds) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(day, app) DO UPDATE SET seconds = seconds + excluded.seconds",
            params![day, name, category, secs],
        );
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySummary {
    day: String,
    active_secs: i64,
    categories: Vec<(String, i64)>,
    apps: Vec<(String, String, i64)>,
    pomodoros: i64,
    chats: i64,
    actions: i64,
}

fn summary(app: &AppHandle, day: &str) -> DaySummary {
    flush(app);
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let conn = db.conn();
    let categories: Vec<(String, i64)> = conn
        .prepare("SELECT category, SUM(seconds) s FROM activity WHERE day = ?1 GROUP BY category ORDER BY s DESC")
        .and_then(|mut st| st.query_map([day], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()))
        .unwrap_or_default();
    let apps: Vec<(String, String, i64)> = conn
        .prepare("SELECT app, category, seconds FROM activity WHERE day = ?1 ORDER BY seconds DESC LIMIT 6")
        .and_then(|mut st| st.query_map([day], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|r| r.flatten().collect()))
        .unwrap_or_default();
    let xp = |kind: &str| -> i64 {
        conn.query_row("SELECT COALESCE(SUM(times), 0) FROM xp_log WHERE day = ?1 AND kind = ?2", params![day, kind], |r| r.get(0))
            .unwrap_or(0)
    };
    DaySummary {
        day: day.to_string(),
        active_secs: categories.iter().map(|(_, s)| s).sum(),
        categories,
        apps,
        pomodoros: xp("pomodoro"),
        chats: xp("chat"),
        actions: xp("action"),
    }
}

#[tauri::command]
pub fn day_summary(app: AppHandle) -> DaySummary {
    summary(&app, &chrono::Local::now().format("%Y-%m-%d").to_string())
}

// ---------- Memória ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    id: i64,
    text: String,
    created_at: String,
}

pub fn memories(app: &AppHandle) -> Vec<Memory> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.conn()
        .prepare("SELECT id, text, created_at FROM memories ORDER BY id")
        .and_then(|mut st| {
            st.query_map([], |r| Ok(Memory { id: r.get(0)?, text: r.get(1)?, created_at: r.get(2)? })).map(|r| r.flatten().collect())
        })
        .unwrap_or_default()
}

pub fn remember(app: &AppHandle, text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() || text.len() > 500 {
        return Err("Diga o que lembrar em uma frase curta.".into());
    }
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let count: i64 = db.conn().query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0)).unwrap_or(0);
    if count >= 60 {
        return Err("A memória está cheia (60 itens). Peça para esquecer algo antes.".into());
    }
    db.conn().execute("INSERT INTO memories (text) VALUES (?1)", [text]).map_err(|e| e.to_string())?;
    let _ = app.emit("memories-changed", ());
    Ok(format!("Guardado: {text}"))
}

pub fn forget(app: &AppHandle, id: i64) -> Result<String, String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let n = db.conn().execute("DELETE FROM memories WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
    let _ = app.emit("memories-changed", ());
    if n == 0 { Err("Não achei essa memória.".into()) } else { Ok("Esquecido.".into()) }
}

/// Bloco do prompt do sistema com o que o Pipo sabe do usuário.
pub fn memory_prompt(app: &AppHandle) -> String {
    let list = memories(app);
    if list.is_empty() {
        return String::new();
    }
    let items: Vec<String> = list.iter().map(|m| format!("- [{}] {}", m.id, m.text)).collect();
    format!("O que você sabe sobre o usuário (ele pediu para você lembrar; use quando ajudar):\n{}", items.join("\n"))
}

#[tauri::command]
pub fn memory_list(app: AppHandle) -> Vec<Memory> {
    memories(&app)
}

#[tauri::command]
pub fn memory_remove(app: AppHandle, id: i64) -> Result<(), String> {
    forget(&app, id).map(|_| ())
}

// ---------- Iniciativa ----------

/// Pode falar agora? Respeita o limite por hora, o intervalo, jogo, foco e ausência.
fn can_speak(app: &AppHandle, idle: u64, gaming: bool) -> bool {
    if gaming || idle >= AWAY || focus::pomodoro_focusing(app) || crate::modes::quiet() {
        return false;
    }
    if !app.state::<AppState>().settings.lock().unwrap().proactive {
        return false;
    }
    with(|s| {
        while s.spoken.front().is_some_and(|t| t.elapsed() > Duration::from_secs(3600)) {
            s.spoken.pop_front();
        }
        s.spoken.len() < MAX_PER_HOUR && s.spoken.back().is_none_or(|t| t.elapsed() >= MIN_GAP)
    })
}

fn speak(app: &AppHandle, kind: &str, title: &str, body: &str, action: Option<&str>) {
    with(|s| s.spoken.push_back(Instant::now()));
    crate::windows::set_pet_state(app, "attentive");
    toast::notify_from(app, "proactive", kind, title, body, action);
}

/// Texto novo na área de transferência: se for um erro, oferece explicar.
pub fn on_clipboard(app: &AppHandle, text: &str) {
    if !clipboard::looks_like_error(text) {
        return;
    }
    let (idle, gaming) = {
        let state = app.state::<AppState>();
        let sense = state.sense.lock().unwrap();
        (sense.idle, sense.gaming)
    };
    let recent = with(|s| s.last_error_tip.is_some_and(|t| t.elapsed() < Duration::from_secs(10 * 60)));
    if recent || !can_speak(app, idle, gaming) {
        return;
    }
    with(|s| s.last_error_tip = Some(Instant::now()));
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let first: String = first.chars().take(90).collect();
    let excerpt: String = text.chars().take(6000).collect();
    speak(
        app,
        "info",
        "Copiou um erro? Posso explicar",
        &first,
        Some(&format!("prompt:Explique este erro e como corrigir:\n\n```\n{excerpt}\n```")),
    );
}

/// Arquivos alterados e idade do último commit de um repositório.
fn git_pending(path: &str) -> Option<(usize, u64)> {
    let run = |args: &[&str]| crate::actions::run("git", &[&["-C", path], args].concat(), None).ok();
    let changed = run(&["status", "--short"])?.lines().filter(|l| !l.trim().is_empty()).count();
    let last: u64 = run(&["log", "-1", "--format=%ct"])?.trim().parse().ok()?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some((changed, now.saturating_sub(last)))
}

/// Chamado pelos sensores a cada 30 s.
pub fn tick(app: &AppHandle, current: Option<&ActiveApp>, idle: u64, gaming: bool) {
    let now = chrono::Local::now();
    let today = now.date_naive();

    // Sessão contínua de uso.
    let session_minutes = with(|s| {
        if idle >= AWAY {
            s.session_since = None;
            s.session_warned = false;
        } else if s.session_since.is_none() {
            s.session_since = Some(Instant::now());
        }
        s.session_since.map_or(0, |t| t.elapsed().as_secs() / 60)
    });

    let settings = app.state::<AppState>().settings.lock().unwrap().clone();

    // Resumo do fim do dia: no horário escolhido, uma vez por dia, se houve uso de verdade.
    let summary_time = chrono::NaiveTime::parse_from_str(settings.day_summary_time.trim(), "%H:%M").ok();
    if settings.proactive
        && !gaming
        && idle < AWAY
        && summary_time.is_some_and(|t| now.time() >= t)
        && with(|s| s.summary_done != Some(today))
    {
        with(|s| s.summary_done = Some(today));
        let data = summary(app, &today.format("%Y-%m-%d").to_string());
        if data.active_secs >= 30 * 60 {
            let hours = data.active_secs as f64 / 3600.0;
            let top = data.categories.first().map(|(c, _)| label(c)).unwrap_or("vários apps");
            speak(app, "info", "Resumo do seu dia", &format!("{hours:.1} h no computador, mais em {top}. Veja na aba Hoje."), Some("view:today"));
            return;
        }
    }

    if !can_speak(app, idle, gaming) {
        return;
    }

    // Muitas mudanças sem commit no projeto aberto.
    let check_commit = with(|s| {
        let due = s.last_commit_check.is_none_or(|t| t.elapsed() >= Duration::from_secs(10 * 60));
        if due {
            s.last_commit_check = Some(Instant::now());
        }
        due
    });
    if check_commit {
        if let Some(name) = current.filter(|c| c.category == Category::Dev).and_then(|c| c.project.clone()) {
            let recent = with(|s| s.commit_nudged.get(&name).is_some_and(|t| t.elapsed() < Duration::from_secs(3 * 3600)));
            if !recent {
                if let Some(project) = projects::recent_projects().into_iter().find(|p| p.name.eq_ignore_ascii_case(&name)) {
                    if let Some((changed, age)) = git_pending(&project.path) {
                        if changed >= 8 && age >= 2 * 3600 {
                            with(|s| s.commit_nudged.insert(name.clone(), Instant::now()));
                            speak(
                                app,
                                "info",
                                &format!("{changed} arquivos sem commit em {name}"),
                                &format!("O último commit foi há {} h. Quer que eu escreva a mensagem?", age / 3600),
                                Some(&format!(
                                    "prompt:Veja as mudanças do projeto {name} ({}) e sugira uma mensagem de commit curta em português.",
                                    project.path
                                )),
                            );
                            return;
                        }
                    }
                }
            }
        }
    }

    // Muito tempo seguido no computador (se os lembretes de pausa estiverem desligados).
    if !settings.break_reminders && session_minutes >= 120 && with(|s| !s.session_warned) {
        with(|s| s.session_warned = true);
        speak(app, "break", "Duas horas seguidas", "Que tal 5 minutos de pausa? Água, alongar e descansar os olhos.", None);
        return;
    }

    // Madrugada.
    let late = now.time() >= chrono::NaiveTime::from_hms_opt(23, 30, 0).unwrap() || now.time() < chrono::NaiveTime::from_hms_opt(5, 0, 0).unwrap();
    let night = if now.time() < chrono::NaiveTime::from_hms_opt(5, 0, 0).unwrap() { today.pred_opt().unwrap_or(today) } else { today };
    if late && with(|s| s.night_warned != Some(night)) {
        with(|s| s.night_warned = Some(night));
        speak(app, "info", "Já está tarde", "Se der, deixe um lembrete do que falta e continue amanhã com a cabeça descansada.", None);
    }
}

pub fn label(category: &str) -> &'static str {
    match category {
        "dev" => "desenvolvimento",
        "docs" => "documentos",
        "browser" => "navegador",
        "live" => "lives",
        "games" => "jogos",
        "chat" => "conversas",
        _ => "outros apps",
    }
}

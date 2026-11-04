//! Lembretes, pomodoro e pausas para água e postura. O laço dos sensores chama
//! `tick` a cada segundo.

use std::time::{Duration, Instant};

use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{windows, AppState};

/// Pausa natural: tanto tempo sem mexer no computador zera a contagem da pausa.
const NATURAL_BREAK: u64 = 5 * 60;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Focus,
    Break,
}

pub struct Pomodoro {
    pub phase: Phase,
    pub ends: Instant,
    pub focus_min: u64,
    pub break_min: u64,
    pub cycle: u32,
}

#[derive(Default)]
pub struct FocusState {
    pub pomodoro: Option<Pomodoro>,
    /// Segundos de uso contínuo desde a última pausa.
    pub active_secs: u64,
    last_reminder_check: Option<Instant>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: i64,
    pub text: String,
    /// Horário local "HH:MM" (com a data se não for hoje).
    pub due: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroStatus {
    pub phase: Phase,
    pub remaining_secs: u64,
    pub cycle: u32,
    pub focus_min: u64,
    pub break_min: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusStatus {
    pub pomodoro: Option<PomodoroStatus>,
    pub reminders: Vec<Reminder>,
    pub breaks_enabled: bool,
    pub break_interval_min: u32,
    pub next_break_secs: Option<u64>,
}

fn notify(app: &AppHandle, kind: &str, title: &str, body: &str) {
    crate::toast::notify(app, kind, title, body, Some("view:actions"));
}

pub fn pomodoro_focusing(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let focus = state.focus.lock().unwrap();
    focus.pomodoro.as_ref().is_some_and(|p| p.phase == Phase::Focus)
}

use chrono::Datelike as _;

/// Data e hora locais em vários formatos: "AAAA-MM-DDTHH:MM" (campo de data do painel),
/// "AAAA-MM-DD HH:MM", "DD/MM/AAAA HH:MM" ou "DD/MM HH:MM" (este ano ou o próximo).
fn parse_date_time(text: &str, now: chrono::DateTime<chrono::Local>) -> Option<chrono::DateTime<chrono::Local>> {
    use chrono::{Datelike, NaiveDateTime};
    let text = text.trim().replace(" às ", " ");
    let full = ["%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M", "%d/%m/%Y %H:%M"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(&text, f).ok());
    let naive = full.or_else(|| {
        let with_year = format!("{} {}", now.year(), text);
        let this_year = NaiveDateTime::parse_from_str(&with_year, "%Y %d/%m %H:%M").ok()?;
        Some(if this_year < now.naive_local() { this_year.with_year(now.year() + 1)? } else { this_year })
    })?;
    naive.and_local_timezone(chrono::Local).single()
}

/// Agenda um lembrete daqui a `minutes` minutos, no horário "HH:MM" (hoje ou amanhã)
/// ou numa data e hora específicas.
pub fn add_reminder(app: &AppHandle, text: &str, minutes: Option<i64>, at: Option<&str>) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("O lembrete está vazio.".into());
    }
    let now = chrono::Local::now();
    let due = match (minutes, at) {
        (Some(m), _) if m > 0 && m <= 7 * 24 * 60 => now + chrono::Duration::minutes(m),
        (_, Some(at)) if at.trim().len() <= 5 => {
            let time = chrono::NaiveTime::parse_from_str(at.trim(), "%H:%M").map_err(|_| "Use o horário no formato HH:MM.")?;
            let today = now.date_naive().and_time(time).and_local_timezone(chrono::Local).single().ok_or("Horário inválido.")?;
            if today > now { today } else { today + chrono::Duration::days(1) }
        }
        (_, Some(at)) => {
            let due = parse_date_time(at, now).ok_or("Use a data no formato DD/MM/AAAA HH:MM.")?;
            if due <= now {
                return Err("Essa data já passou.".into());
            }
            if due > now + chrono::Duration::days(3 * 365) {
                return Err("Escolha uma data nos próximos 3 anos.".into());
            }
            due
        }
        _ => return Err("Diga em quantos minutos (1 a 10080), em que horário (HH:MM) ou em que data (DD/MM/AAAA HH:MM).".into()),
    };
    let utc = due.with_timezone(&chrono::Utc).format("%Y-%m-%d %H:%M:%S").to_string();
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.conn()
        .execute("INSERT INTO reminders (text, due_at) VALUES (?1, ?2)", params![text, utc])
        .map_err(|e| e.to_string())?;
    let _ = app.emit("focus-changed", ());
    Ok(if due.year_ce() == now.year_ce() { due.format("%d/%m %H:%M") } else { due.format("%d/%m/%Y %H:%M") }.to_string())
}

fn local_due(utc: &str) -> String {
    let Ok(naive) = chrono::NaiveDateTime::parse_from_str(utc, "%Y-%m-%d %H:%M:%S") else { return utc.to_string() };
    let local = naive.and_utc().with_timezone(&chrono::Local);
    let today = chrono::Local::now().date_naive();
    if local.date_naive() == today {
        local.format("%H:%M").to_string()
    } else if local.format("%Y").to_string() == today.format("%Y").to_string() {
        local.format("%d/%m %H:%M").to_string()
    } else {
        local.format("%d/%m/%Y %H:%M").to_string()
    }
}

pub fn reminders(app: &AppHandle) -> Vec<Reminder> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let Ok(mut stmt) = db.conn().prepare("SELECT id, text, due_at FROM reminders WHERE done = 0 ORDER BY due_at") else {
        return Vec::new();
    };
    stmt.query_map([], |r| {
        let due: String = r.get(2)?;
        Ok(Reminder { id: r.get(0)?, text: r.get(1)?, due: local_due(&due) })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

pub fn start_pomodoro(app: &AppHandle, focus_min: u64, break_min: u64) -> Result<(), String> {
    if !(1..=180).contains(&focus_min) || !(1..=60).contains(&break_min) {
        return Err("Foco de 1 a 180 minutos e pausa de 1 a 60.".into());
    }
    let state = app.state::<AppState>();
    state.focus.lock().unwrap().pomodoro = Some(Pomodoro {
        phase: Phase::Focus,
        ends: Instant::now() + Duration::from_secs(focus_min * 60),
        focus_min,
        break_min,
        cycle: 1,
    });
    let _ = app.emit("focus-changed", ());
    Ok(())
}

pub fn stop_pomodoro(app: &AppHandle) {
    app.state::<AppState>().focus.lock().unwrap().pomodoro = None;
    let _ = app.emit("focus-changed", ());
}

pub fn status(app: &AppHandle) -> FocusStatus {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    let (pomodoro, active) = {
        let focus = state.focus.lock().unwrap();
        let pomodoro = focus.pomodoro.as_ref().map(|p| PomodoroStatus {
            phase: p.phase,
            remaining_secs: p.ends.saturating_duration_since(Instant::now()).as_secs(),
            cycle: p.cycle,
            focus_min: p.focus_min,
            break_min: p.break_min,
        });
        (pomodoro, focus.active_secs)
    };
    let interval = settings.break_interval_min as u64 * 60;
    FocusStatus {
        pomodoro,
        reminders: reminders(app),
        breaks_enabled: settings.break_reminders,
        break_interval_min: settings.break_interval_min,
        next_break_secs: settings.break_reminders.then(|| interval.saturating_sub(active)),
    }
}

/// Chamado a cada segundo: lembretes vencidos, fases do pomodoro e pausas.
pub fn tick(app: &AppHandle, idle: u64, gaming: bool) {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    let mut changed = false;

    // Lembretes: consulta o banco a cada 5 s. Avisam mesmo durante jogos.
    let check = {
        let mut focus = state.focus.lock().unwrap();
        let due = focus.last_reminder_check.is_none_or(|t| t.elapsed() >= Duration::from_secs(5));
        if due {
            focus.last_reminder_check = Some(Instant::now());
        }
        due
    };
    if check {
        let due: Vec<(i64, String)> = {
            let db = state.db.lock().unwrap();
            let rows: Vec<(i64, String)> = db
                .conn()
                .prepare("SELECT id, text FROM reminders WHERE done = 0 AND due_at <= datetime('now')")
                .and_then(|mut stmt| stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()))
                .unwrap_or_default();
            for (id, _) in &rows {
                let _ = db.conn().execute("UPDATE reminders SET done = 1 WHERE id = ?1", params![id]);
            }
            rows
        };
        for (_, text) in &due {
            notify(app, "reminder", "Lembrete", text);
            windows::set_pet_state(app, "alert");
            changed = true;
        }
    }

    // Pomodoro
    let mut completed = false;
    let transition = {
        let mut focus = state.focus.lock().unwrap();
        match focus.pomodoro.as_mut() {
            Some(p) if Instant::now() >= p.ends => {
                let message = match p.phase {
                    Phase::Focus => {
                        completed = true;
                        p.phase = Phase::Break;
                        p.ends = Instant::now() + Duration::from_secs(p.break_min * 60);
                        format!("Fim do foco {}. Pausa de {} minutos.", p.cycle, p.break_min)
                    }
                    Phase::Break => {
                        p.phase = Phase::Focus;
                        p.cycle += 1;
                        p.ends = Instant::now() + Duration::from_secs(p.focus_min * 60);
                        format!("Pausa encerrada. Foco {} de {} minutos.", p.cycle, p.focus_min)
                    }
                };
                Some(message)
            }
            _ => None,
        }
    };
    if let Some(message) = transition {
        if !gaming {
            notify(app, "pomodoro", "Pomodoro", &message);
        }
        windows::set_pet_state(app, "done");
        changed = true;
    }
    if completed {
        crate::life::award(app, "pomodoro", 1);
    }

    // Pausas: conta o tempo de uso; ficar parado 5 min já vale como pausa.
    let remind_break = {
        let mut focus = state.focus.lock().unwrap();
        if idle >= NATURAL_BREAK {
            focus.active_secs = 0;
        } else if idle < 60 {
            focus.active_secs += 1;
        }
        let interval = settings.break_interval_min.max(10) as u64 * 60;
        let due = settings.break_reminders && focus.active_secs >= interval && focus.pomodoro.is_none();
        if due {
            focus.active_secs = 0;
        }
        due
    };
    if remind_break && !gaming {
        notify(app, "break", "Hora de uma pausa", "Você está há um tempo no computador. Beba água, ajuste a postura e descanse os olhos.");
        windows::set_pet_state(app, "alert");
    }

    if changed {
        let _ = app.emit("focus-changed", ());
    }
}

#[tauri::command]
pub fn focus_status(app: AppHandle) -> FocusStatus {
    status(&app)
}

#[tauri::command]
pub fn reminder_add(app: AppHandle, text: String, minutes: Option<i64>, at: Option<String>) -> Result<String, String> {
    add_reminder(&app, &text, minutes, at.as_deref())
}

#[tauri::command]
pub fn reminder_delete(app: AppHandle, id: i64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.conn().execute("DELETE FROM reminders WHERE id = ?1", params![id]).map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pomodoro_start(app: AppHandle, focus_min: Option<u64>, break_min: Option<u64>) -> Result<(), String> {
    start_pomodoro(&app, focus_min.unwrap_or(25), break_min.unwrap_or(5))
}

#[tauri::command]
pub fn pomodoro_stop(app: AppHandle) {
    stop_pomodoro(&app);
}

#[cfg(test)]
mod tests {
    use super::parse_date_time;
    use chrono::TimeZone;

    #[test]
    fn le_datas_dos_lembretes() {
        let now = chrono::Local.with_ymd_and_hms(2026, 11, 3, 20, 0, 0).unwrap();
        let fmt = |s: &str| parse_date_time(s, now).map(|d| d.format("%Y-%m-%d %H:%M").to_string());
        assert_eq!(fmt("2026-12-25T09:30").as_deref(), Some("2026-12-25 09:30"));
        assert_eq!(fmt("25/12/2026 09:30").as_deref(), Some("2026-12-25 09:30"));
        assert_eq!(fmt("25/12 às 09:30").as_deref(), Some("2026-12-25 09:30"));
        // Data deste ano que já passou vai para o ano que vem.
        assert_eq!(fmt("10/01 08:00").as_deref(), Some("2027-01-10 08:00"));
        assert!(fmt("amanhã").is_none());
    }
}

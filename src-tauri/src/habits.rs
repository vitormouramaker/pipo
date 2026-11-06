//! Fase 9.1: hábitos e metas com sequência de dias.
//!
//! Cada hábito tem uma meta por dia (1 = "feito / não feito", ou uma quantidade, como
//! 8 copos de água), os dias da semana em que vale e um horário opcional de lembrete.
//! Cumprir a meta dá XP ao Pipo; a sequência conta só os dias em que o hábito vale
//! (um dia de folga não quebra a sequência). Tudo fica no banco do Pipo (vai no backup).

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

/// Dias da semana, de segunda (0) a domingo (6).
pub const WEEKDAYS: [&str; 7] = ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"];
/// Sequências que viram comemoração (e XP extra).
const MILESTONES: [i64; 6] = [3, 7, 14, 30, 60, 100];
/// Até quantos dias para trás a sequência é contada.
const HISTORY: i64 = 400;
/// Horário do "não quebre a sequência" para quem ainda não cumpriu o hábito do dia.
const NUDGE_AT: &str = "21:00";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Habit {
    #[serde(default)]
    pub id: i64,
    pub name: String,
    /// Meta por dia (1 = feito ou não).
    #[serde(default = "one")]
    pub target: i64,
    /// Unidade da meta ("copos", "páginas", "min"); vazia na meta de 1.
    #[serde(default)]
    pub unit: String,
    /// Dias em que vale (0 = segunda … 6 = domingo). Vazio = todo dia.
    #[serde(default)]
    pub days: Vec<u8>,
    /// Lembrete "HH:MM" (vazio = sem lembrete).
    #[serde(default)]
    pub remind_at: String,
}

fn one() -> i64 {
    1
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HabitView {
    #[serde(flatten)]
    habit: Habit,
    /// Quanto já foi feito hoje.
    today: i64,
    done: bool,
    /// O hábito vale hoje?
    scheduled: bool,
    streak: i64,
    best: i64,
    /// Últimos 7 dias (o último é hoje): "done", "miss", "off" (não vale) ou "todo" (hoje, ainda não).
    week: Vec<&'static str>,
}

impl HabitView {
    pub fn id(&self) -> i64 {
        self.habit.id
    }
}

fn scheduled(days: &[u8], day: NaiveDate) -> bool {
    days.is_empty() || days.contains(&(day.weekday().num_days_from_monday() as u8))
}

/// Sequência atual: dias seguidos (só os dias em que vale) com a meta cumprida.
/// Hoje ainda não cumprido não quebra a sequência; antes da criação, para.
fn streak_at(days: &[u8], created: NaiveDate, today: NaiveDate, done: &dyn Fn(NaiveDate) -> bool) -> i64 {
    let mut count = 0;
    let mut day = today;
    for _ in 0..HISTORY {
        if day < created {
            break;
        }
        if scheduled(days, day) {
            if done(day) {
                count += 1;
            } else if day != today {
                break;
            }
        }
        day = match day.pred_opt() {
            Some(d) => d,
            None => break,
        };
    }
    count
}

/// Melhor sequência desde a criação.
fn best_at(days: &[u8], created: NaiveDate, today: NaiveDate, done: &dyn Fn(NaiveDate) -> bool) -> i64 {
    let (mut best, mut run) = (0, 0);
    let mut day = created.max(today - chrono::Duration::days(HISTORY));
    while day <= today {
        if scheduled(days, day) {
            if done(day) {
                run += 1;
                best = best.max(run);
            } else if day != today {
                run = 0;
            }
        }
        day = match day.succ_opt() {
            Some(d) => d,
            None => break,
        };
    }
    best
}

fn parse_days(text: &str) -> Vec<u8> {
    text.split(',').filter_map(|d| d.trim().parse::<u8>().ok()).filter(|d| *d < 7).collect()
}

fn day_key(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

struct Row {
    habit: Habit,
    created: NaiveDate,
}

fn rows(conn: &Connection) -> Vec<Row> {
    let Ok(mut stmt) = conn.prepare("SELECT id, name, target, unit, days, remind_at, date(created_at, 'localtime') FROM habits WHERE archived = 0 ORDER BY id") else {
        return Vec::new();
    };
    stmt.query_map([], |r| {
        let created: String = r.get(6)?;
        Ok(Row {
            habit: Habit { id: r.get(0)?, name: r.get(1)?, target: r.get::<_, i64>(2)?.max(1), unit: r.get(3)?, days: parse_days(&r.get::<_, String>(4)?), remind_at: r.get(5)? },
            created: created.parse().unwrap_or_else(|_| Local::now().date_naive()),
        })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

/// Quanto foi feito em cada dia (últimos HISTORY dias) de um hábito.
fn amounts(conn: &Connection, id: i64) -> std::collections::HashMap<NaiveDate, i64> {
    let since = day_key(Local::now().date_naive() - chrono::Duration::days(HISTORY + 1));
    let Ok(mut stmt) = conn.prepare("SELECT day, amount FROM habit_log WHERE habit_id = ?1 AND day >= ?2") else { return Default::default() };
    stmt.query_map(params![id, since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .map(|rows| rows.flatten().filter_map(|(d, a)| Some((d.parse().ok()?, a))).collect())
        .unwrap_or_default()
}

fn view(conn: &Connection, row: &Row, today: NaiveDate) -> HabitView {
    let h = &row.habit;
    let log = amounts(conn, h.id);
    let done = |d: NaiveDate| log.get(&d).copied().unwrap_or(0) >= h.target;
    let week = (0..7)
        .rev()
        .map(|back| {
            let d = today - chrono::Duration::days(back);
            if d < row.created || !scheduled(&h.days, d) {
                "off"
            } else if done(d) {
                "done"
            } else if d == today {
                "todo"
            } else {
                "miss"
            }
        })
        .collect();
    HabitView {
        today: log.get(&today).copied().unwrap_or(0),
        done: done(today),
        scheduled: scheduled(&h.days, today),
        streak: streak_at(&h.days, row.created, today, &done),
        best: best_at(&h.days, row.created, today, &done),
        week,
        habit: h.clone(),
    }
}

pub fn list_in(conn: &Connection) -> Vec<HabitView> {
    let today = Local::now().date_naive();
    rows(conn).iter().map(|r| view(conn, r, today)).collect()
}

pub fn list(app: &AppHandle) -> Vec<HabitView> {
    list_in(app.state::<AppState>().db.lock().unwrap().conn())
}

/// Maior sequência atual entre os hábitos (para a conquista).
pub fn best_streak(conn: &Connection) -> i64 {
    list_in(conn).iter().map(|h| h.streak).max().unwrap_or(0)
}

fn changed(app: &AppHandle) {
    let _ = app.emit("habits-changed", ());
}

fn clean(mut h: Habit) -> Result<Habit, String> {
    h.name = h.name.trim().chars().take(60).collect();
    if h.name.is_empty() {
        return Err("Dê um nome ao hábito.".into());
    }
    h.target = h.target.clamp(1, 1000);
    h.unit = h.unit.trim().chars().take(20).collect();
    h.days.sort_unstable();
    h.days.dedup();
    h.days.retain(|d| *d < 7);
    if h.days.len() == 7 {
        h.days.clear();
    }
    h.remind_at = h.remind_at.trim().to_string();
    if !h.remind_at.is_empty() && chrono::NaiveTime::parse_from_str(&h.remind_at, "%H:%M").is_err() {
        return Err("Use o horário do lembrete no formato HH:MM.".into());
    }
    Ok(h)
}

/// Cria (id 0) ou altera um hábito.
pub fn save(app: &AppHandle, habit: Habit) -> Result<i64, String> {
    let h = clean(habit)?;
    let days = h.days.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(",");
    let id = {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        let conn = db.conn();
        if h.id > 0 {
            conn.execute("UPDATE habits SET name = ?1, target = ?2, unit = ?3, days = ?4, remind_at = ?5 WHERE id = ?6", params![h.name, h.target, h.unit, days, h.remind_at, h.id])
                .map_err(|e| e.to_string())?;
            h.id
        } else {
            conn.execute("INSERT INTO habits (name, target, unit, days, remind_at) VALUES (?1, ?2, ?3, ?4, ?5)", params![h.name, h.target, h.unit, days, h.remind_at])
                .map_err(|e| e.to_string())?;
            conn.last_insert_rowid()
        }
    };
    changed(app);
    Ok(id)
}

/// Apaga um hábito (fica guardado como arquivado, com o histórico).
pub fn delete(app: &AppHandle, id: i64) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        db.conn().execute("UPDATE habits SET archived = 1 WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
    }
    changed(app);
    Ok(())
}

/// Soma `delta` ao que foi feito hoje (negativo desfaz). Devolve o estado novo do hábito.
pub fn log(app: &AppHandle, id: i64, delta: i64) -> Result<HabitView, String> {
    let today = Local::now().date_naive();
    let (before, after) = {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        let conn = db.conn();
        let row = rows(conn).into_iter().find(|r| r.habit.id == id).ok_or("Esse hábito não existe mais.")?;
        let before = view(conn, &row, today);
        let amount = (before.today + delta).clamp(0, 100_000);
        conn.execute(
            "INSERT INTO habit_log (habit_id, day, amount) VALUES (?1, ?2, ?3) ON CONFLICT(habit_id, day) DO UPDATE SET amount = excluded.amount",
            params![id, day_key(today), amount],
        )
        .map_err(|e| e.to_string())?;
        (before, view(conn, &row, today))
    };
    if !before.done && after.done {
        crate::journal::add(app, "habito", &after.habit.name, "", "");
        crate::life::award(app, "habit", 1);
        crate::windows::set_pet_state(app, "done");
        if MILESTONES.contains(&after.streak) && after.streak > before.streak {
            crate::life::award(app, "habit-streak", 1);
            crate::toast::notify_from(
                app,
                "pipo",
                "achievement",
                &format!("{} dias seguidos: {}!", after.streak, after.habit.name),
                &format!("Sua melhor sequência é de {} dias. Continue assim!", after.best),
                Some("view:today"),
            );
        }
    }
    changed(app);
    Ok(after)
}

/// Acha um hábito pelo nome (sem acento e sem diferenciar maiúsculas).
pub fn find(app: &AppHandle, name: &str) -> Option<HabitView> {
    let fold = |s: &str| crate::documents::fold(s).trim().to_string();
    let wanted = fold(name.trim());
    let all = list(app);
    all.iter()
        .find(|h| fold(&h.habit.name) == wanted)
        .or_else(|| all.iter().find(|h| fold(&h.habit.name).contains(&wanted) || wanted.contains(&fold(&h.habit.name))))
        .cloned()
}

/// Texto curto "Água 3/8 copos · 🔥 5" para o chat, o Telegram e os avisos.
pub fn line(h: &HabitView) -> String {
    let progress = if h.habit.target > 1 {
        format!("{}/{}{}", h.today, h.habit.target, if h.habit.unit.is_empty() { String::new() } else { format!(" {}", h.habit.unit) })
    } else if h.done {
        "feito".into()
    } else {
        "pendente".into()
    };
    let rest = if h.scheduled { "" } else { " (hoje é folga)" };
    format!("{} (id {}): {progress}{rest} · sequência {} (melhor {})", h.habit.name, h.habit.id, h.streak, h.best)
}

static REMINDED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn first_time(key: String) -> bool {
    REMINDED.lock().unwrap().get_or_insert_with(HashSet::new).insert(key)
}

/// Lembra no horário de cada hábito e, às 21h, avisa quem está para perder a sequência.
fn tick(app: &AppHandle) {
    let now = Local::now();
    let hm = now.format("%H:%M").to_string();
    let today = day_key(now.date_naive());
    for h in list(app).into_iter().filter(|h| h.scheduled && !h.done) {
        let left = if h.habit.target > 1 { format!("Faltam {}{}.", h.habit.target - h.today, if h.habit.unit.is_empty() { String::new() } else { format!(" {}", h.habit.unit) }) } else { String::new() };
        if !h.habit.remind_at.is_empty() && h.habit.remind_at == hm && first_time(format!("{}:{today}:hora", h.habit.id)) {
            let streak = if h.streak > 0 { format!(" Sequência: {} dia(s).", h.streak) } else { String::new() };
            crate::windows::set_pet_state(app, "attentive");
            crate::toast::notify_from(app, "reminder", "reminder", &format!("Hora de: {}", h.habit.name), &format!("{left}{streak}").trim().to_string(), Some(&format!("habit:{}", h.habit.id)));
        }
        if hm == NUDGE_AT && h.streak >= 2 && first_time(format!("{}:{today}:noite", h.habit.id)) {
            crate::toast::notify_from(
                app,
                "reminder",
                "reminder",
                &format!("Não quebre a sequência: {}", h.habit.name),
                &format!("São {} dias seguidos. {left}", h.streak).trim().to_string(),
                Some(&format!("habit:{}", h.habit.id)),
            );
        }
    }
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || loop {
        tick(&app);
        std::thread::sleep(Duration::from_secs(20));
    });
}

#[tauri::command]
pub fn habits_list(app: AppHandle) -> Vec<HabitView> {
    list(&app)
}

#[tauri::command]
pub fn habit_save(app: AppHandle, habit: Habit) -> Result<i64, String> {
    save(&app, habit)
}

#[tauri::command]
pub fn habit_delete(app: AppHandle, id: i64) -> Result<(), String> {
    delete(&app, id)
}

#[tauri::command]
pub fn habit_log(app: AppHandle, id: i64, delta: i64) -> Result<HabitView, String> {
    log(&app, id, delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn sequencia_pula_folgas_e_espera_o_dia_de_hoje() {
        // Seg a sex. Hoje é sexta 06/11/2026 e ainda não fez; fez seg a qui (02 a 05).
        let weekdays = [0, 1, 2, 3, 4];
        let done_days = [d("2026-10-30"), d("2026-11-02"), d("2026-11-03"), d("2026-11-04"), d("2026-11-05")];
        let done = |x: NaiveDate| done_days.contains(&x);
        // Sexta 30/10 também conta: o fim de semana não vale e não quebra.
        assert_eq!(streak_at(&weekdays, d("2026-10-01"), d("2026-11-06"), &done), 5);
        // Todo dia: o sábado 31/10 sem fazer quebra.
        assert_eq!(streak_at(&[], d("2026-10-01"), d("2026-11-06"), &done), 4);
        // Ontem sem fazer quebra a sequência.
        let gap = |x: NaiveDate| x == d("2026-11-04");
        assert_eq!(streak_at(&[], d("2026-10-01"), d("2026-11-06"), &gap), 0);
        assert_eq!(best_at(&weekdays, d("2026-10-29"), d("2026-11-06"), &done), 5);
    }

    #[test]
    fn limpa_o_habito() {
        let h = clean(Habit { id: 0, name: "  Água  ".into(), target: 0, unit: " copos ".into(), days: vec![6, 0, 0, 9], remind_at: "08:00".into() }).unwrap();
        assert_eq!((h.name.as_str(), h.target, h.unit.as_str(), h.days.clone()), ("Água", 1, "copos", vec![0, 6]));
        let all = clean(Habit { id: 0, name: "x".into(), target: 1, unit: String::new(), days: (0..7).collect(), remind_at: String::new() }).unwrap();
        assert!(all.days.is_empty());
        assert!(clean(Habit { id: 0, name: "x".into(), target: 1, unit: String::new(), days: vec![], remind_at: "8h".into() }).is_err());
    }
}

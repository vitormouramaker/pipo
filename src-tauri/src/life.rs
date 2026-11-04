//! Fase 5: a vida do Pipo. Ele ganha XP com o uso (conversas, ações, pomodoros,
//! buscas, comandos de voz e um "bom dia" por dia), sobe de nível, evolui de
//! estágio, libera acessórios e conquistas. Tudo fica no banco local.

use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{windows, AppState};

/// Quanto vale cada coisa e o limite por dia (para conversa à toa não virar farm).
fn reward(kind: &str) -> (i64, Option<i64>) {
    match kind {
        "chat" => (5, Some(100)),
        "action" => (10, Some(200)),
        "web" => (3, Some(60)),
        "pomodoro" => (25, None),
        "voice" => (8, Some(80)),
        "briefing" => (10, Some(10)),
        "checkin" => (15, Some(15)),
        _ => (0, Some(0)),
    }
}

/// XP para ir do nível `level` ao seguinte.
fn need(level: i64) -> i64 {
    100 + 40 * (level - 1)
}

/// (nível, XP dentro do nível, XP para o próximo)
fn level_of(total: i64) -> (i64, i64, i64) {
    let (mut level, mut rest) = (1, total);
    while rest >= need(level) {
        rest -= need(level);
        level += 1;
    }
    (level, rest, need(level))
}

pub const STAGES: &[(i64, &str, &str)] = &[
    (1, "Alpha", "Recém-instalado, ainda aprendendo seus hábitos."),
    (5, "Beta", "Já conhece sua rotina e ganhou uma borda própria."),
    (10, "Release", "Estável: borda iluminada e mais acessórios."),
    (20, "LTS", "Suporte de longo prazo. Companheiro de verdade."),
];

pub const ACCESSORIES: &[(&str, &str, i64)] = &[
    ("antenna", "Antena", 3),
    ("headset", "Fone", 6),
    ("bowtie", "Gravata-borboleta", 10),
    ("crown", "Coroa", 20),
];

struct Achievement {
    id: &'static str,
    title: &'static str,
    detail: &'static str,
}

const ACHIEVEMENTS: &[Achievement] = &[
    Achievement { id: "first-chat", title: "Olá, Pipo", detail: "Primeira conversa." },
    Achievement { id: "chat-100", title: "Papo longo", detail: "100 mensagens no chat." },
    Achievement { id: "first-action", title: "Mãos à obra", detail: "Primeira ação feita pelo Pipo." },
    Achievement { id: "action-50", title: "Operador", detail: "50 ações feitas pelo Pipo." },
    Achievement { id: "pomodoro-10", title: "Foco total", detail: "10 pomodoros concluídos." },
    Achievement { id: "web-25", title: "Pesquisador", detail: "25 buscas na web." },
    Achievement { id: "first-voice", title: "Fala comigo", detail: "Primeiro comando de voz." },
    Achievement { id: "streak-7", title: "Rotina", detail: "7 dias seguidos com o Pipo." },
    Achievement { id: "days-30", title: "Veterano", detail: "30 dias de uso." },
    Achievement { id: "level-10", title: "Release estável", detail: "Chegou ao nível 10." },
];

fn stage_of(level: i64) -> (usize, &'static str) {
    let i = STAGES.iter().rposition(|(min, _, _)| level >= *min).unwrap_or(0);
    (i, STAGES[i].1)
}

/// Dias seguidos (até hoje ou ontem) com "bom dia" registrado.
fn streak(conn: &rusqlite::Connection) -> i64 {
    let mut stmt = match conn.prepare("SELECT DISTINCT day FROM xp_log WHERE kind = 'checkin' ORDER BY day DESC LIMIT 400") {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let days: Vec<chrono::NaiveDate> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map(|rows| rows.flatten().filter_map(|d| d.parse().ok()).collect())
        .unwrap_or_default();
    let today = chrono::Local::now().date_naive();
    let Some(first) = days.first() else { return 0 };
    if (today - *first).num_days() > 1 {
        return 0;
    }
    let mut count = 1;
    for pair in days.windows(2) {
        if (pair[0] - pair[1]).num_days() == 1 {
            count += 1;
        } else {
            break;
        }
    }
    count
}

fn count(conn: &rusqlite::Connection, kind: &str) -> i64 {
    conn.query_row("SELECT COALESCE(SUM(times), 0) FROM xp_log WHERE kind = ?1", [kind], |r| r.get(0)).unwrap_or(0)
}

fn total_xp(conn: &rusqlite::Connection) -> i64 {
    conn.query_row("SELECT COALESCE(SUM(xp), 0) FROM xp_log", [], |r| r.get(0)).unwrap_or(0)
}

fn unlocked(conn: &rusqlite::Connection) -> Vec<(String, String)> {
    let Ok(mut stmt) = conn.prepare("SELECT id, unlocked_at FROM achievements ORDER BY unlocked_at") else { return vec![] };
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// (quanto já foi feito, meta) de cada conquista.
fn progress(conn: &rusqlite::Connection, id: &str) -> (i64, i64) {
    let days = || conn.query_row("SELECT COUNT(DISTINCT day) FROM xp_log", [], |r| r.get(0)).unwrap_or(0);
    let (done, goal) = match id {
        "first-chat" => (count(conn, "chat"), 1),
        "chat-100" => (count(conn, "chat"), 100),
        "first-action" => (count(conn, "action"), 1),
        "action-50" => (count(conn, "action"), 50),
        "pomodoro-10" => (count(conn, "pomodoro"), 10),
        "web-25" => (count(conn, "web"), 25),
        "first-voice" => (count(conn, "voice"), 1),
        "streak-7" => (streak(conn), 7),
        "days-30" => (days(), 30),
        "level-10" => (level_of(total_xp(conn)).0, 10),
        _ => (0, 1),
    };
    (done.min(goal), goal)
}

/// Conquistas que acabaram de ser alcançadas.
fn check_achievements(conn: &rusqlite::Connection) -> Vec<&'static Achievement> {
    let have: Vec<String> = unlocked(conn).into_iter().map(|(id, _)| id).collect();
    let reached = |id: &str| {
        let (done, goal) = progress(conn, id);
        done >= goal
    };
    let new: Vec<&Achievement> = ACHIEVEMENTS.iter().filter(|a| !have.iter().any(|h| h == a.id) && reached(a.id)).collect();
    for a in &new {
        let _ = conn.execute("INSERT OR IGNORE INTO achievements (id) VALUES (?1)", [a.id]);
    }
    new
}

/// Registra `times` ocorrências de `kind` e dá o XP correspondente (respeitando o limite do dia).
pub fn award(app: &AppHandle, kind: &str, times: i64) {
    if times <= 0 {
        return;
    }
    let (each, cap) = reward(kind);
    let state = app.state::<AppState>();
    let (before, after, achievements) = {
        let db = state.db.lock().unwrap();
        let conn = db.conn();
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        let today: i64 = conn
            .query_row("SELECT COALESCE(SUM(xp), 0) FROM xp_log WHERE kind = ?1 AND day = ?2", params![kind, day], |r| r.get(0))
            .unwrap_or(0);
        let xp = match cap {
            Some(cap) => (each * times).min((cap - today).max(0)),
            None => each * times,
        };
        let before = total_xp(conn);
        if conn
            .execute("INSERT INTO xp_log (day, kind, times, xp) VALUES (?1, ?2, ?3, ?4)", params![day, kind, times, xp])
            .is_err()
        {
            return;
        }
        (level_of(before).0, level_of(before + xp).0, check_achievements(conn))
    };

    if after > before {
        let (stage_before, _) = stage_of(before);
        let (stage_after, name) = stage_of(after);
        let unlocked: Vec<&str> = ACCESSORIES.iter().filter(|(_, _, lvl)| *lvl > before && *lvl <= after).map(|(_, n, _)| *n).collect();
        let mut body = format!("Subi para o nível {after}.");
        if stage_after > stage_before {
            body.push_str(&format!(" Evoluí para o estágio {name}!"));
        }
        if !unlocked.is_empty() {
            body.push_str(&format!(" Novo acessório: {}.", unlocked.join(", ")));
        }
        crate::toast::notify(app, "level", &format!("Nível {after}!"), &body, Some("view:pipo"));
        windows::set_pet_state(app, "levelup");
    }
    for a in achievements {
        crate::toast::notify(app, "achievement", &format!("Conquista: {}", a.title), a.detail, Some("view:pipo"));
    }
    let _ = app.emit("life-changed", ());
}

/// "Bom dia": o primeiro uso ativo do dia vale XP e conta para a sequência.
pub fn checkin_if_new_day(app: &AppHandle) {
    let state = app.state::<AppState>();
    let done = {
        let db = state.db.lock().unwrap();
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        db.conn()
            .query_row("SELECT 1 FROM xp_log WHERE kind = 'checkin' AND day = ?1", [day], |_| Ok(()))
            .is_ok()
    };
    if !done {
        award(app, "checkin", 1);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageView {
    name: &'static str,
    from_level: i64,
    detail: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessoryView {
    id: &'static str,
    name: &'static str,
    level: i64,
    unlocked: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AchievementView {
    id: &'static str,
    title: &'static str,
    detail: &'static str,
    unlocked_at: Option<String>,
    progress: i64,
    goal: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifeStatus {
    total_xp: i64,
    level: i64,
    level_xp: i64,
    level_need: i64,
    stage: usize,
    stage_name: &'static str,
    stages: Vec<StageView>,
    accessories: Vec<AccessoryView>,
    achievements: Vec<AchievementView>,
    streak: i64,
    days: i64,
    since: Option<String>,
    chats: i64,
    actions: i64,
    pomodoros: i64,
    searches: i64,
    voice: i64,
}

/// Nível atual (para o relatório semanal).
pub fn level_now(conn: &rusqlite::Connection) -> i64 {
    level_of(total_xp(conn)).0
}

#[tauri::command]
pub fn life_status(app: AppHandle) -> LifeStatus {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let conn = db.conn();
    let total = total_xp(conn);
    let (level, level_xp, level_need) = level_of(total);
    let (stage, stage_name) = stage_of(level);
    let have = unlocked(conn);
    LifeStatus {
        total_xp: total,
        level,
        level_xp,
        level_need,
        stage,
        stage_name,
        stages: STAGES.iter().map(|(from_level, name, detail)| StageView { name, from_level: *from_level, detail }).collect(),
        accessories: ACCESSORIES
            .iter()
            .map(|(id, name, lvl)| AccessoryView { id, name, level: *lvl, unlocked: level >= *lvl })
            .collect(),
        achievements: ACHIEVEMENTS
            .iter()
            .map(|a| {
                let (progress, goal) = progress(conn, a.id);
                AchievementView {
                    id: a.id,
                    title: a.title,
                    detail: a.detail,
                    unlocked_at: have.iter().find(|(id, _)| id == a.id).map(|(_, at)| at.clone()),
                    progress,
                    goal,
                }
            })
            .collect(),
        streak: streak(conn),
        days: conn.query_row("SELECT COUNT(DISTINCT day) FROM xp_log", [], |r| r.get(0)).unwrap_or(0),
        since: conn.query_row("SELECT MIN(day) FROM xp_log", [], |r| r.get(0)).unwrap_or(None),
        chats: count(conn, "chat"),
        actions: count(conn, "action"),
        pomodoros: count(conn, "pomodoro"),
        searches: count(conn, "web"),
        voice: count(conn, "voice"),
    }
}

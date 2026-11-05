//! Fase 7.6: League of Legends. Lê o cliente do LoL pela API local dele (porta e senha do
//! arquivo "lockfile") e a partida pela Live Client Data API (porta 2999). As duas só
//! respondem no próprio PC; nada disso é enviado para fora, a não ser o pedido de dica
//! (campeão, posição e adversários) para a Claude, se o usuário deixar.
//!
//! - Seleção de campeões: quando você trava o campeão, chega uma dica curta para a partida.
//! - Na partida: avisa 1 minuto antes do Dragão e do Barão voltarem (contando das mortes).
//! - No fim: resultado, KDA e farm por minuto, com atalho para pedir uma análise.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LolStatus {
    running: bool,
    /// Fase do cliente: "Lobby", "ChampSelect", "InProgress", "EndOfGame"…
    phase: String,
    champion: Option<String>,
    position: Option<String>,
    tip: Option<String>,
    last_game: Option<String>,
}

static STATUS: Mutex<Option<LolStatus>> = Mutex::new(None);

fn update(app: &AppHandle, change: impl FnOnce(&mut LolStatus)) {
    let changed = {
        let mut guard = STATUS.lock().unwrap();
        let status = guard.get_or_insert_with(LolStatus::default);
        let before = serde_json::to_string(status).unwrap_or_default();
        change(status);
        before != serde_json::to_string(status).unwrap_or_default()
    };
    if changed {
        let _ = app.emit("lol-changed", ());
    }
}

/// Pastas onde o LoL costuma estar (e a que o Riot Client registrou).
fn install_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let program_data = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into());
    if let Ok(text) = std::fs::read_to_string(PathBuf::from(program_data).join("Riot Games").join("RiotClientInstalls.json")) {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            for (path, _) in v["associated_client"].as_object().into_iter().flatten() {
                if path.to_lowercase().contains("league") {
                    dirs.push(PathBuf::from(path));
                }
            }
        }
    }
    for drive in ["C", "D", "E"] {
        dirs.push(PathBuf::from(format!("{drive}:\\Riot Games\\League of Legends")));
    }
    dirs
}

/// "LeagueClient:pid:porta:senha:https" -> (porta, senha). Só existe com o cliente aberto.
fn parse_lockfile(text: &str) -> Option<(u16, String)> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    (parts.len() >= 5).then(|| Some((parts[2].parse().ok()?, parts[3].to_string()))).flatten()
}

fn lockfile(dirs: &[PathBuf]) -> Option<(u16, String)> {
    dirs.iter().find_map(|d| std::fs::read_to_string(d.join("lockfile")).ok().and_then(|t| parse_lockfile(&t)))
}

/// As duas APIs usam certificado próprio do LoL, só em 127.0.0.1.
fn local_client() -> reqwest::Client {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(6))
        .build()
        .unwrap_or_default()
}

async fn lcu(client: &reqwest::Client, port: u16, password: &str, path: &str) -> Option<Value> {
    let response = client.get(format!("https://127.0.0.1:{port}{path}")).basic_auth("riot", Some(password)).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json().await.ok()
}

async fn live(client: &reqwest::Client, path: &str) -> Option<Value> {
    client.get(format!("https://127.0.0.1:2999/liveclientdata/{path}")).send().await.ok()?.json().await.ok()
}

fn position_name(p: &str) -> &'static str {
    match p.to_lowercase().as_str() {
        "top" => "topo",
        "jungle" => "selva",
        "middle" | "mid" => "meio",
        "bottom" | "bot" => "atirador",
        "utility" | "support" => "suporte",
        _ => "",
    }
}

#[derive(Default)]
struct Game {
    /// Campeão para o qual a dica já foi pedida.
    tipped: Option<i64>,
    champion: Option<String>,
    position: String,
    /// Último evento da partida já visto.
    last_event: i64,
    /// Avisos agendados: (tempo de jogo em segundos, texto).
    timers: Vec<(f64, String)>,
    ended: bool,
}

async fn champ_select(app: &AppHandle, client: &reqwest::Client, port: u16, password: &str, names: &mut HashMap<i64, String>, game: &mut Game) {
    let Some(session) = lcu(client, port, password, "/lol-champ-select/v1/session").await else { return };
    let me = session["localPlayerCellId"].as_i64();
    let Some(mine) = session["myTeam"].as_array().into_iter().flatten().find(|p| p["cellId"].as_i64() == me) else { return };
    let champion = mine["championId"].as_i64().filter(|&c| c > 0);
    let locked = session["actions"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|turn| turn.as_array().into_iter().flatten())
        .any(|a| a["actorCellId"].as_i64() == me && a["type"] == "pick" && a["completed"] == true);
    let Some(champion) = champion.filter(|_| locked) else { return };
    if game.tipped == Some(champion) {
        return;
    }
    game.tipped = Some(champion);
    if names.is_empty() {
        if let Some(list) = lcu(client, port, password, "/lol-game-data/assets/v1/champion-summary.json").await {
            for c in list.as_array().into_iter().flatten() {
                if let (Some(id), Some(name)) = (c["id"].as_i64(), c["name"].as_str()) {
                    names.insert(id, name.to_string());
                }
            }
        }
    }
    let name = names.get(&champion).cloned().unwrap_or_else(|| format!("campeão {champion}"));
    let position = position_name(mine["assignedPosition"].as_str().unwrap_or("")).to_string();
    let enemies: Vec<String> = session["theirTeam"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["championId"].as_i64().filter(|&c| c > 0))
        .filter_map(|id| names.get(&id).cloned())
        .collect();
    game.champion = Some(name.clone());
    game.position = position.clone();
    update(app, |s| {
        s.champion = Some(name.clone());
        s.position = (!position.is_empty()).then(|| position.clone());
        s.tip = None;
    });
    if !app.state::<AppState>().settings.lock().unwrap().lol_tips {
        return;
    }
    let role = if position.is_empty() { String::new() } else { format!(" no {position}") };
    let against = if enemies.is_empty() { String::new() } else { format!(" contra {}", enemies.join(", ")) };
    let web = app.state::<AppState>().settings.lock().unwrap().lol_web_build;
    let app = app.clone();
    let more = format!("prompt:Me explica melhor como jogar de {name}{role}{against}: runas, build e plano de jogo.");
    tauri::async_runtime::spawn(async move {
        let system = "Você é o Pipo, um assistente que entende de League of Legends. Responda em português do Brasil, direto ao ponto.";
        // Com a busca ligada, a dica usa a build e as runas do patch atual (sites de estatísticas).
        let patch = if web { current_patch().await } else { None };
        let prompt = match &patch {
            Some(patch) => format!(
                "Vou jogar League of Legends de {name}{role}{against}, no patch {patch}. Pesquise a build e as runas mais usadas e com \
                 melhor taxa de vitória neste patch para essa rota (sites como u.gg, op.gg ou lolalytics) e me dê em no máximo 5 linhas \
                 curtas: runas principais, itens iniciais e núcleo da build, plano para a fase de rotas e o maior cuidado contra esses \
                 adversários. Sem introdução e sem links."
            ),
            None => format!(
                "Vou jogar League of Legends de {name}{role}{against}. Me dê uma dica rápida para esta partida em no máximo 4 linhas curtas: \
                 runas/itens iniciais, plano para a fase de rotas e o maior cuidado. Sem introdução."
            ),
        };
        let answer = if patch.is_some() { crate::ai::ask_web(&app, system, &prompt, 700, 2).await } else { crate::ai::ask(&app, system, &prompt, 350).await };
        match answer {
            Ok(tip) => {
                update(&app, |s| s.tip = Some(tip.clone()));
                crate::toast::notify_from(&app, "lol", "lol", &format!("Dica: {name}{role}"), &tip, Some(&more));
            }
            Err(e) => update(&app, |s| s.tip = Some(format!("Não consegui a dica: {e}"))),
        }
    });
}

async fn in_game(app: &AppHandle, client: &reqwest::Client, game: &mut Game) {
    let Some(stats) = live(client, "gamestats").await else { return };
    let now = stats["gameTime"].as_f64().unwrap_or(0.0);
    if let Some(events) = live(client, "eventdata").await {
        for e in events["Events"].as_array().into_iter().flatten() {
            let id = e["EventID"].as_i64().unwrap_or(-1);
            if id <= game.last_event && game.last_event >= 0 {
                continue;
            }
            game.last_event = id;
            let at = e["EventTime"].as_f64().unwrap_or(now);
            // Dragão volta 5 min depois de morto; Barão, 6 min. O aviso vem 1 min antes
            // (e não sai para mortes antigas, se o Pipo abriu no meio da partida).
            let respawn = match e["EventName"].as_str().unwrap_or("") {
                "GameStart" => {
                    game.timers.clear();
                    None
                }
                "DragonKill" => Some((300.0, "O Dragão volta em 1 minuto.")),
                "BaronKill" => Some((360.0, "O Barão volta em 1 minuto.")),
                _ => None,
            };
            if let Some((after, text)) = respawn {
                let warn = at + after - 60.0;
                if warn > now {
                    game.timers.push((warn, text.into()));
                }
            }
        }
    }
    let due: Vec<String> = game.timers.iter().filter(|(t, _)| *t <= now).map(|(_, text)| text.clone()).collect();
    game.timers.retain(|(t, _)| *t > now);
    for text in due {
        crate::toast::notify_from(app, "lol", "lol", "Objetivo", &text, None);
    }
}

async fn end_of_game(app: &AppHandle, client: &reqwest::Client, port: u16, password: &str, game: &mut Game) {
    if game.ended {
        return;
    }
    let Some(block) = lcu(client, port, password, "/lol-end-of-game/v1/eog-stats-block").await else { return };
    game.ended = true;
    let me = &block["localPlayer"];
    let stats = &me["stats"];
    let n = |k: &str| stats[k].as_i64().unwrap_or(0);
    let minutes = (block["gameLength"].as_f64().unwrap_or(0.0) / 60.0).max(1.0);
    let cs = (n("MINIONS_KILLED") + n("NEUTRAL_MINIONS_KILLED")) as f64 / minutes;
    let won = n("WIN") == 1
        || block["teams"].as_array().into_iter().flatten().any(|t| {
            t["isWinningTeam"] == true && t["players"].as_array().into_iter().flatten().any(|p| p["summonerId"] == me["summonerId"] && !me["summonerId"].is_null())
        });
    let champion = me["championName"].as_str().map(String::from).or_else(|| game.champion.clone()).unwrap_or_else(|| "seu campeão".into());
    let (k, d, a) = (n("CHAMPIONS_KILLED"), n("NUM_DEATHS"), n("ASSISTS"));
    let summary = format!("{champion}: {k}/{d}/{a} · {cs:.1} de farm por minuto · {minutes:.0} min");
    update(app, |s| s.last_game = Some(format!("{} — {summary}", if won { "Vitória" } else { "Derrota" })));
    {
        // Guarda a partida (relatório semanal e, depois, o histórico do LoL).
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        let _ = db.conn().execute(
            "INSERT OR IGNORE INTO lol_games (day, champion, position, won, kills, deaths, assists, cs_min, minutes, game_id, played_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                chrono::Local::now().format("%Y-%m-%d").to_string(),
                champion,
                game.position,
                won as i64,
                k,
                d,
                a,
                cs,
                minutes,
                block["gameId"].as_i64(),
                chrono::Utc::now().timestamp_millis()
            ],
        );
    }
    let _ = app.emit("lol-changed", ());
    if !won {
        tilt_check(app);
    }
    let role = if game.position.is_empty() { String::new() } else { format!(" no {}", game.position) };
    let prompt = format!(
        "prompt:Analise minha última partida de League of Legends: {champion}{role}, {} com {k}/{d}/{a} e {cs:.1} de farm por minuto em {minutes:.0} minutos. Me dê 3 coisas para melhorar.",
        if won { "vitória" } else { "derrota" }
    );
    crate::toast::notify_from(app, "lol", "lol", if won { "Vitória!" } else { "Derrota" }, &summary, Some(&prompt));
}

// ---------- Fase 8.4: patch atual, histórico e tilt ----------

/// Versão atual do jogo pelo Data Dragon (ex.: "15.22"), guardada por algumas horas.
static PATCH: Mutex<Option<(std::time::Instant, String)>> = Mutex::new(None);

async fn current_patch() -> Option<String> {
    if let Some((at, v)) = PATCH.lock().unwrap().clone() {
        if at.elapsed() < Duration::from_secs(6 * 3600) {
            return Some(v);
        }
    }
    let versions: Value = crate::world::http().get("https://ddragon.leagueoflegends.com/api/versions.json").send().await.ok()?.json().await.ok()?;
    let full = versions.as_array()?.first()?.as_str()?.to_string();
    let short = full.split('.').take(2).collect::<Vec<_>>().join(".");
    *PATCH.lock().unwrap() = Some((std::time::Instant::now(), short.clone()));
    Some(short)
}

async fn ensure_names(client: &reqwest::Client, port: u16, password: &str, names: &mut HashMap<i64, String>) {
    if !names.is_empty() {
        return;
    }
    if let Some(list) = lcu(client, port, password, "/lol-game-data/assets/v1/champion-summary.json").await {
        for c in list.as_array().into_iter().flatten() {
            if let (Some(id), Some(name)) = (c["id"].as_i64(), c["name"].as_str()) {
                names.insert(id, name.to_string());
            }
        }
    }
}

fn lane_name(lane: &str, role: &str, queue: i64) -> String {
    if queue == 450 {
        return "ARAM".into();
    }
    match (lane, role) {
        ("BOTTOM", "DUO_SUPPORT") => "suporte".into(),
        (lane, _) => position_name(lane).to_string(),
    }
}

/// Importa as últimas partidas do cliente (uma vez por vez que o cliente abre).
async fn sync_history(app: &AppHandle, client: &reqwest::Client, port: u16, password: &str, names: &mut HashMap<i64, String>) {
    let Some(data) = lcu(client, port, password, "/lol-match-history/v1/products/lol/current-summoner/matches?begIndex=0&endIndex=19").await else { return };
    ensure_names(client, port, password, names).await;
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    for g in data["games"]["games"].as_array().into_iter().flatten() {
        let Some(me) = g["participants"].as_array().and_then(|p| p.first()) else { continue };
        let stats = &me["stats"];
        let n = |k: &str| stats[k].as_i64().unwrap_or(0);
        let minutes = (g["gameDuration"].as_f64().unwrap_or(0.0) / 60.0).max(1.0);
        let cs = (n("totalMinionsKilled") + n("neutralMinionsKilled")) as f64 / minutes;
        let created = g["gameCreation"].as_i64().unwrap_or(0);
        let day = chrono::DateTime::from_timestamp_millis(created).map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string()).unwrap_or_default();
        let champion = me["championId"].as_i64().and_then(|id| names.get(&id).cloned()).unwrap_or_else(|| "?".into());
        let position = lane_name(me["timeline"]["lane"].as_str().unwrap_or(""), me["timeline"]["role"].as_str().unwrap_or(""), g["queueId"].as_i64().unwrap_or(0));
        let _ = db.conn().execute(
            "INSERT OR IGNORE INTO lol_games (day, champion, position, won, kills, deaths, assists, cs_min, minutes, game_id, played_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![day, champion, position, stats["win"] == true, n("kills"), n("deaths"), n("assists"), cs, minutes, g["gameId"].as_i64(), created],
        );
    }
    drop(db);
    let _ = app.emit("lol-changed", ());
}

/// Três derrotas seguidas nas últimas 4 horas: sugere uma pausa (uma vez por sequência).
fn tilt_check(app: &AppHandle) {
    if !app.state::<AppState>().settings.lock().unwrap().lol_tilt_alert {
        return;
    }
    let since = chrono::Utc::now().timestamp_millis() - 4 * 3600 * 1000;
    let recent: Vec<bool> = {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        db.conn()
            .prepare("SELECT won FROM lol_games WHERE played_at >= ?1 ORDER BY played_at DESC LIMIT 3")
            .and_then(|mut st| st.query_map([since], |r| r.get::<_, bool>(0)).map(|rows| rows.flatten().collect()))
            .unwrap_or_default()
    };
    if tilted(&recent) {
        crate::toast::notify_from(
            app,
            "lol",
            "lol",
            "Três derrotas seguidas",
            "Hora de uma pausa de 15 minutos: água, alongar, respirar. Fila nova com a cabeça fria rende mais.",
            Some("prompt:Perdi 3 partidas seguidas no LoL. Me ajuda a entender o que pode estar dando errado e como sair do tilt?"),
        );
    }
}

fn tilted(recent: &[bool]) -> bool {
    recent.len() == 3 && recent.iter().all(|won| !won)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRow {
    day: String,
    champion: String,
    position: String,
    won: bool,
    kills: i64,
    deaths: i64,
    assists: i64,
    cs_min: f64,
    minutes: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    games: Vec<GameRow>,
    wins: usize,
    /// (campeão, partidas) mais jogados.
    top: Vec<(String, usize)>,
    kda: f64,
    cs_min: f64,
}

#[tauri::command]
pub fn lol_history(app: AppHandle) -> History {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let games: Vec<GameRow> = db
        .conn()
        .prepare(
            "SELECT day, champion, position, won, kills, deaths, assists, cs_min, minutes FROM lol_games
             ORDER BY COALESCE(played_at, 0) DESC, id DESC LIMIT 20",
        )
        .and_then(|mut st| {
            st.query_map([], |r| {
                Ok(GameRow {
                    day: r.get(0)?,
                    champion: r.get(1)?,
                    position: r.get(2)?,
                    won: r.get(3)?,
                    kills: r.get(4)?,
                    deaths: r.get(5)?,
                    assists: r.get(6)?,
                    cs_min: r.get(7)?,
                    minutes: r.get(8)?,
                })
            })
            .map(|rows| rows.flatten().collect())
        })
        .unwrap_or_default();
    let mut count: HashMap<String, usize> = HashMap::new();
    for g in &games {
        *count.entry(g.champion.clone()).or_default() += 1;
    }
    let mut top: Vec<(String, usize)> = count.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    top.truncate(3);
    let (k, d, a, cs) = games.iter().fold((0, 0, 0, 0.0), |acc, g| (acc.0 + g.kills, acc.1 + g.deaths, acc.2 + g.assists, acc.3 + g.cs_min));
    let n = games.len().max(1) as f64;
    History {
        wins: games.iter().filter(|g| g.won).count(),
        kda: (k + a) as f64 / (d.max(1)) as f64,
        cs_min: cs / n,
        top,
        games,
    }
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let client = local_client();
        let dirs = install_dirs();
        let mut names: HashMap<i64, String> = HashMap::new();
        let mut game = Game { last_event: -1, ..Default::default() };
        // O histórico é importado uma vez por vez que o cliente abre.
        let mut synced = false;
        loop {
            let enabled = app.state::<AppState>().settings.lock().unwrap().lol_enabled;
            let Some((port, password)) = enabled.then(|| lockfile(&dirs)).flatten() else {
                if STATUS.lock().unwrap().as_ref().is_some_and(|s| s.running) {
                    update(&app, |s| *s = LolStatus { last_game: s.last_game.clone(), ..Default::default() });
                }
                synced = false;
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            };
            let phase = lcu(&client, port, &password, "/lol-gameflow/v1/gameflow-phase").await.and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            update(&app, |s| {
                s.running = true;
                s.phase = phase.clone();
            });
            if !synced && !phase.is_empty() {
                synced = true;
                sync_history(&app, &client, port, &password, &mut names).await;
            }
            match phase.as_str() {
                "ChampSelect" => champ_select(&app, &client, port, &password, &mut names, &mut game).await,
                "InProgress" => in_game(&app, &client, &mut game).await,
                "PreEndOfGame" | "EndOfGame" => end_of_game(&app, &client, port, &password, &mut game).await,
                "GameStart" | "Reconnect" | "WaitingForStats" => {}
                _ => {
                    // Lobby, fila, etc.: prepara a próxima partida.
                    if game.tipped.is_some() || game.ended {
                        game = Game { last_event: -1, ..Default::default() };
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}

#[tauri::command]
pub fn lol_status() -> LolStatus {
    STATUS.lock().unwrap().clone().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilt_so_com_tres_derrotas() {
        assert!(tilted(&[false, false, false]));
        assert!(!tilted(&[false, true, false]));
        assert!(!tilted(&[false, false]));
        assert_eq!(lane_name("BOTTOM", "DUO_SUPPORT", 420), "suporte");
        assert_eq!(lane_name("MIDDLE", "SOLO", 420), "meio");
        assert_eq!(lane_name("MIDDLE", "SOLO", 450), "ARAM");
    }

    #[test]
    fn le_o_lockfile() {
        assert_eq!(parse_lockfile("LeagueClient:1234:54321:s3nh4:https"), Some((54321, "s3nh4".into())));
        assert_eq!(parse_lockfile("lixo"), None);
        assert_eq!(position_name("UTILITY"), "suporte");
    }
}

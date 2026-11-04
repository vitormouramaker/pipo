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
    let prompt = format!(
        "Vou jogar League of Legends de {name}{role}{against}. Me dê uma dica rápida para esta partida em no máximo 4 linhas curtas: \
         runas/itens iniciais, plano para a fase de rotas e o maior cuidado. Sem introdução."
    );
    let app = app.clone();
    let more = format!("prompt:Me explica melhor como jogar de {name}{role}{against}: runas, build e plano de jogo.");
    tauri::async_runtime::spawn(async move {
        let system = "Você é o Pipo, um assistente que entende de League of Legends. Responda em português do Brasil, direto ao ponto.";
        match crate::ai::ask(&app, system, &prompt, 350).await {
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
    let role = if game.position.is_empty() { String::new() } else { format!(" no {}", game.position) };
    let prompt = format!(
        "prompt:Analise minha última partida de League of Legends: {champion}{role}, {} com {k}/{d}/{a} e {cs:.1} de farm por minuto em {minutes:.0} minutos. Me dê 3 coisas para melhorar.",
        if won { "vitória" } else { "derrota" }
    );
    crate::toast::notify_from(app, "lol", "lol", if won { "Vitória!" } else { "Derrota" }, &summary, Some(&prompt));
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let client = local_client();
        let dirs = install_dirs();
        let mut names: HashMap<i64, String> = HashMap::new();
        let mut game = Game { last_event: -1, ..Default::default() };
        loop {
            let enabled = app.state::<AppState>().settings.lock().unwrap().lol_enabled;
            let Some((port, password)) = enabled.then(|| lockfile(&dirs)).flatten() else {
                if STATUS.lock().unwrap().as_ref().is_some_and(|s| s.running) {
                    update(&app, |s| *s = LolStatus { last_game: s.last_game.clone(), ..Default::default() });
                }
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            };
            let phase = lcu(&client, port, &password, "/lol-gameflow/v1/gameflow-phase").await.and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            update(&app, |s| {
                s.running = true;
                s.phase = phase.clone();
            });
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
    fn le_o_lockfile() {
        assert_eq!(parse_lockfile("LeagueClient:1234:54321:s3nh4:https"), Some((54321, "s3nh4".into())));
        assert_eq!(parse_lockfile("lixo"), None);
        assert_eq!(position_name("UTILITY"), "suporte");
    }
}

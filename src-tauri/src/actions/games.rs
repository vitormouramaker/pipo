//! Jogos instalados na Steam, na Epic Games e no Riot Client, com favoritos.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use super::{run, shell_open};
use crate::AppState;

const FAVORITES: &str = "favorite_games";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    /// "steam:<appid>", "epic:<namespace>:<item>:<app>" ou "riot:<produto>"
    pub id: String,
    pub name: String,
    pub launcher: String,
    pub favorite: bool,
}

/// Valor entre aspas depois de uma chave nos arquivos .vdf/.acf da Steam.
fn vdf_values<'a>(text: &'a str, key: &str) -> Vec<&'a str> {
    let needle = format!("\"{key}\"");
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix(&needle)?;
            let value = rest.trim().strip_prefix('"')?.strip_suffix('"')?;
            Some(value)
        })
        .collect()
}

fn steam_root() -> Option<PathBuf> {
    let out = run("reg", &["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"], None).ok()?;
    let line = out.lines().find(|l| l.contains("SteamPath"))?;
    let path = line.split("REG_SZ").nth(1)?.trim();
    Some(PathBuf::from(path.replace('/', "\\")))
}

fn steam_games() -> Vec<Game> {
    let Some(root) = steam_root() else { return Vec::new() };
    let mut libraries = vec![root.join("steamapps")];
    if let Ok(vdf) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
        libraries.extend(vdf_values(&vdf, "path").into_iter().map(|p| PathBuf::from(p.replace("\\\\", "\\")).join("steamapps")));
    }
    // A biblioteca principal também aparece no libraryfolders.vdf: lê cada pasta uma vez só.
    let mut seen_libs = std::collections::HashSet::new();
    libraries.retain(|lib| seen_libs.insert(lib.to_string_lossy().to_lowercase()));
    let skip = ["Redistributable", "Steamworks", "Proton", "SteamVR", "Steam Linux Runtime"];
    let mut seen_ids = std::collections::HashSet::new();
    let mut games = Vec::new();
    for lib in libraries {
        let Ok(entries) = std::fs::read_dir(&lib) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Ok(acf) = std::fs::read_to_string(entry.path()) else { continue };
            let (Some(appid), Some(title)) = (vdf_values(&acf, "appid").first().copied(), vdf_values(&acf, "name").first().copied())
            else {
                continue;
            };
            if appid == "228980" || skip.iter().any(|s| title.contains(s)) || !seen_ids.insert(appid.to_string()) {
                continue;
            }
            games.push(Game { id: format!("steam:{appid}"), name: title.to_string(), launcher: "Steam".into(), favorite: false });
        }
    }
    games
}

fn epic_games() -> Vec<Game> {
    let dir = Path::new(r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "item"))
        .filter_map(|e| serde_json::from_str::<Value>(&std::fs::read_to_string(e.path()).ok()?).ok())
        .filter_map(|v| {
            let s = |k: &str| v[k].as_str().map(String::from);
            Some(Game {
                id: format!("epic:{}:{}:{}", s("CatalogNamespace")?, s("CatalogItemId")?, s("AppName")?),
                name: s("DisplayName")?,
                launcher: "Epic Games".into(),
                favorite: false,
            })
        })
        .collect()
}

fn riot_client() -> Option<PathBuf> {
    let raw = std::fs::read_to_string(r"C:\ProgramData\Riot Games\RiotClientInstalls.json").ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    let path = PathBuf::from(json["rc_default"].as_str()?);
    path.exists().then_some(path)
}

fn riot_games() -> Vec<Game> {
    if riot_client().is_none() {
        return Vec::new();
    }
    let meta = Path::new(r"C:\ProgramData\Riot Games\Metadata");
    [("league_of_legends", "League of Legends"), ("valorant", "VALORANT"), ("bacon", "Legends of Runeterra")]
        .iter()
        .filter(|(product, _)| meta.join(format!("{product}.live")).exists())
        .map(|(product, name)| Game { id: format!("riot:{product}"), name: name.to_string(), launcher: "Riot".into(), favorite: false })
        .collect()
}

fn favorites(app: &AppHandle) -> Vec<String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.get_setting(FAVORITES).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn list(app: &AppHandle) -> Vec<Game> {
    let favs = favorites(app);
    let mut games: Vec<Game> = steam_games().into_iter().chain(epic_games()).chain(riot_games()).collect();
    for g in &mut games {
        g.favorite = favs.contains(&g.id);
    }
    games.sort_by(|a, b| b.favorite.cmp(&a.favorite).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    games
}

pub fn launch(id: &str) -> Result<(), String> {
    let safe = |s: &str| s.chars().all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c));
    let parts: Vec<&str> = id.split(':').collect();
    match parts.as_slice() {
        ["steam", appid] if appid.chars().all(|c| c.is_ascii_digit()) => shell_open(&format!("steam://rungameid/{appid}")),
        ["epic", ns, item, app] if safe(ns) && safe(item) && safe(app) => {
            shell_open(&format!("com.epicgames.launcher://apps/{ns}%3A{item}%3A{app}?action=launch&silent=true"))
        }
        ["riot", product] if safe(product) => {
            let client = riot_client().ok_or("Riot Client não encontrado.")?;
            std::process::Command::new(client)
                .args([format!("--launch-product={product}"), "--launch-patchline=live".into()])
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        _ => Err("Jogo não reconhecido.".into()),
    }
}

#[tauri::command]
pub fn games_list(app: AppHandle) -> Vec<Game> {
    list(&app)
}

#[tauri::command]
pub fn game_launch(id: String) -> Result<(), String> {
    launch(&id)
}

#[tauri::command]
pub fn game_favorite(app: AppHandle, id: String, favorite: bool) -> Result<(), String> {
    let mut favs = favorites(&app);
    favs.retain(|f| f != &id);
    if favorite {
        favs.push(id);
    }
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.set_setting(FAVORITES, &serde_json::to_string(&favs).unwrap_or_default()).map_err(|e| e.to_string())
}

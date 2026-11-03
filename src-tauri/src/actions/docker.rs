//! Containers do Docker.

use serde::Serialize;
use serde_json::Value;

use super::run;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub ports: String,
}

fn docker(args: &[&str]) -> Result<String, String> {
    run("docker", args, None).map_err(|e| {
        if e.contains("error during connect") || e.contains("cannot find the file specified") || e.contains("pipe") {
            "O Docker Desktop não está rodando.".into()
        } else {
            e
        }
    })
}

pub fn containers() -> Result<Vec<Container>, String> {
    let output = docker(&["ps", "-a", "--format", "{{json .}}"])?;
    Ok(output
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|v| {
            let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
            Container { id: s("ID"), name: s("Names"), image: s("Image"), state: s("State"), status: s("Status"), ports: s("Ports") }
        })
        .collect())
}

pub fn action(id: &str, action: &str) -> Result<(), String> {
    if !["start", "stop", "restart"].contains(&action) {
        return Err(format!("Ação inválida: {action}"));
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c)) {
        return Err("Container inválido.".into());
    }
    docker(&[action, id]).map(|_| ())
}

#[tauri::command]
pub fn docker_list() -> Result<Vec<Container>, String> {
    containers()
}

#[tauri::command]
pub fn docker_action(id: String, action: String) -> Result<(), String> {
    self::action(&id, &action)
}

//! Portas TCP em uso (servidores locais) e encerramento do processo dono.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use super::run;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    pub port: u16,
    pub pid: u32,
    pub process: String,
    pub address: String,
}

/// Processos do Windows que sempre têm portas abertas e não interessam aqui.
const SYSTEM: &[&str] = &["svchost.exe", "lsass.exe", "wininit.exe", "services.exe", "spoolsv.exe", "system", "registry"];

/// PID -> nome, pelo tasklist (que lê inclusive os processos de serviço).
fn process_names() -> HashMap<u32, String> {
    let out = run("tasklist", &["/FO", "CSV", "/NH"], None).unwrap_or_default();
    out.lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split("\",\"").collect();
            let name = cols.first()?.trim_start_matches('"').to_string();
            let pid = cols.get(1)?.parse().ok()?;
            Some((pid, name))
        })
        .collect()
}

/// Lê "netstat -ano" (linhas LISTENING) e junta IPv4/IPv6 da mesma porta e processo.
pub fn listening() -> Result<Vec<Port>, String> {
    let names = process_names();
    let output = run("netstat", &["-ano", "-p", "TCP"], None)? + &run("netstat", &["-ano", "-p", "TCPv6"], None).unwrap_or_default();
    let mut ports: BTreeMap<(u16, u32), Port> = BTreeMap::new();
    for line in output.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 || cols[3] != "LISTENING" {
            continue;
        }
        let Some((address, port)) = cols[1].rsplit_once(':') else { continue };
        let (Ok(port), Ok(pid)) = (port.parse::<u16>(), cols[4].parse::<u32>()) else { continue };
        let process = names.get(&pid).cloned().unwrap_or_else(|| format!("PID {pid}"));
        if pid <= 4 || SYSTEM.contains(&process.to_lowercase().as_str()) {
            continue; // processos do próprio Windows
        }
        ports.entry((port, pid)).or_insert_with(|| Port {
            port,
            pid,
            process,
            address: address.trim_matches(['[', ']']).to_string(),
        });
    }
    Ok(ports.into_values().collect())
}

pub fn kill(pid: u32) -> Result<(), String> {
    if pid <= 4 || pid == std::process::id() {
        return Err("Esse processo não pode ser encerrado pelo Pipo.".into());
    }
    run("taskkill", &["/PID", &pid.to_string(), "/T", "/F"], None).map(|_| ())
}

#[tauri::command]
pub fn ports_list() -> Result<Vec<Port>, String> {
    listening()
}

#[tauri::command]
pub fn process_kill(pid: u32) -> Result<(), String> {
    kill(pid)
}

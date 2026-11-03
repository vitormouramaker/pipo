//! Scripts rodados em segundo plano: o Pipo acompanha e avisa quando terminam
//! (ideal para build e testes).

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Stdio;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use super::{command, now_hm, projects};
use crate::{windows, AppState};

const MAX_LINES: usize = 300;
const MAX_JOBS: usize = 20;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: u64,
    pub project: String,
    pub command: String,
    /// "running", "ok", "failed" ou "stopped"
    pub status: String,
    pub exit_code: Option<i32>,
    pub started_at: String,
    pub duration_secs: Option<u64>,
    pub output: Vec<String>,
    #[serde(skip)]
    pub pid: u32,
}

#[derive(Default)]
pub struct Jobs {
    pub list: Vec<Job>,
    next_id: u64,
}

fn with_job(app: &AppHandle, id: u64, f: impl FnOnce(&mut Job)) {
    let state = app.state::<AppState>();
    let mut jobs = state.jobs.lock().unwrap();
    if let Some(job) = jobs.list.iter_mut().find(|j| j.id == id) {
        f(job);
    }
}

fn pipe_lines(app: AppHandle, id: u64, reader: impl Read + Send + 'static) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::new();
        while reader.read_until(b'\n', &mut buf).unwrap_or(0) > 0 {
            let line = String::from_utf8_lossy(&buf).trim_end().to_string();
            buf.clear();
            with_job(&app, id, |job| {
                job.output.push(line);
                if job.output.len() > MAX_LINES {
                    job.output.remove(0);
                }
            });
        }
    })
}

pub fn start(app: &AppHandle, path: &str, script: &str) -> Result<u64, String> {
    let dir = projects::ensure_dir(path)?;
    let cmdline = projects::script_command(path, script)?;
    let mut child = command("cmd")
        .args(["/C", &cmdline])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Não consegui iniciar: {e}"))?;

    let project = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let id = {
        let state = app.state::<AppState>();
        let mut jobs = state.jobs.lock().unwrap();
        jobs.next_id += 1;
        let id = jobs.next_id;
        jobs.list.insert(0, Job {
            id,
            project: project.clone(),
            command: cmdline.clone(),
            status: "running".into(),
            exit_code: None,
            started_at: now_hm(),
            duration_secs: None,
            output: Vec::new(),
            pid: child.id(),
        });
        // Descarta as mais antigas já terminadas.
        while jobs.list.len() > MAX_JOBS {
            match jobs.list.iter().rposition(|j| j.status != "running") {
                Some(i) => drop(jobs.list.remove(i)),
                None => break,
            }
        }
        id
    };
    let _ = app.emit("jobs-changed", ());

    let readers = [
        child.stdout.take().map(|o| pipe_lines(app.clone(), id, o)),
        child.stderr.take().map(|e| pipe_lines(app.clone(), id, e)),
    ];
    let app = app.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        let status = child.wait();
        for reader in readers.into_iter().flatten() {
            let _ = reader.join();
        }
        let secs = started.elapsed().as_secs();
        let code = status.ok().and_then(|s| s.code());
        let mut stopped = false;
        with_job(&app, id, |job| {
            stopped = job.status == "stopped";
            if !stopped {
                job.status = if code == Some(0) { "ok" } else { "failed" }.into();
            }
            job.exit_code = code;
            job.duration_secs = Some(secs);
        });
        let _ = app.emit("jobs-changed", ());
        if stopped {
            return;
        }
        let ok = code == Some(0);
        windows::set_pet_state(&app, if ok { "done" } else { "alert" });
        let body = if ok {
            format!("{cmdline} terminou em {secs} s ({project}).")
        } else {
            format!("{cmdline} falhou (código {}) em {project}.", code.map_or("?".into(), |c| c.to_string()))
        };
        let _ = app.notification().builder().title(if ok { "Pipo · concluído" } else { "Pipo · falhou" }).body(body).show();
    });
    Ok(id)
}

pub fn list(app: &AppHandle) -> Vec<Job> {
    app.state::<AppState>().jobs.lock().unwrap().list.clone()
}

pub fn stop(app: &AppHandle, id: u64) -> Result<(), String> {
    let pid = {
        let state = app.state::<AppState>();
        let mut jobs = state.jobs.lock().unwrap();
        let job = jobs.list.iter_mut().find(|j| j.id == id).ok_or("Tarefa não encontrada.")?;
        if job.status != "running" {
            return Ok(());
        }
        job.status = "stopped".into();
        job.pid
    };
    // /T encerra também os processos filhos (ex.: o node iniciado pelo npm).
    super::run("taskkill", &["/PID", &pid.to_string(), "/T", "/F"], None).map(|_| ())
}

#[tauri::command]
pub fn jobs_list(app: AppHandle) -> Vec<Job> {
    list(&app)
}

#[tauri::command]
pub fn job_start(app: AppHandle, path: String, script: String) -> Result<u64, String> {
    start(&app, &path, &script)
}

#[tauri::command]
pub fn job_stop(app: AppHandle, id: u64) -> Result<(), String> {
    stop(&app, id)
}

#[tauri::command]
pub fn jobs_clear(app: AppHandle) {
    app.state::<AppState>().jobs.lock().unwrap().list.retain(|j| j.status == "running");
}

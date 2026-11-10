//! Otimização de memória RAM, do jeito que ajuda de verdade.
//!
//! "Limpadores de RAM" que esvaziam todos os processos de uma vez só baixam o número por um
//! instante: o Windows recarrega tudo assim que o app volta a ser usado, e o PC fica mais
//! lento. Aqui o Pipo:
//! - mostra quem está usando a memória e há quanto tempo cada app não é usado;
//! - "Otimizar agora": devolve ao Windows a memória de apps PARADOS (sem uso há 5 min ou
//!   mais), nunca a do app em uso, do jogo nem de processos do sistema;
//! - sugere fechar apps pesados parados há mais de 30 min (fechamento gentil, como clicar no
//!   X: o app pode pedir para salvar);
//! - se a pessoa ligar, faz isso sozinho quando a memória passa do limite (no máximo a cada 10 min).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{AppHandle, Manager};

use crate::AppState;

/// Apps usados há menos que isso não são mexidos.
const RECENT: Duration = Duration::from_secs(5 * 60);
/// Parado há mais que isso e pesado: vira sugestão de fechar.
const CLOSE_AFTER: Duration = Duration::from_secs(30 * 60);
const HEAVY: u64 = 300 * 1024 * 1024;
const AUTO_EVERY: Duration = Duration::from_secs(10 * 60);

/// Processos do sistema (e do próprio Windows) em que o Pipo não mexe.
const PROTECTED: &[&str] = &[
    "system", "registry", "smss", "csrss", "wininit", "winlogon", "services", "lsass", "svchost", "dwm", "explorer",
    "fontdrvhost", "memory compression", "audiodg", "msmpeng", "nissrv", "securityhealthservice", "sihost", "ctfmon",
    "taskhostw", "runtimebroker", "searchhost", "searchindexer", "startmenuexperiencehost", "shellexperiencehost",
    "textinputhost", "lockapp", "dllhost", "conhost", "spoolsv", "wudfhost", "idle", "system idle process", "smartscreen",
    "applicationframehost", "systemsettings", "widgets", "widgetservice", "phoneexperiencehost", "mpdefendercoreservice",
];

struct Track {
    started: Instant,
    /// Último momento em que cada app (nome do executável, minúsculo, sem .exe) esteve em uso.
    last_active: HashMap<String, Instant>,
    last_run: Option<(chrono::DateTime<chrono::Local>, u64, usize)>,
    last_auto: Option<Instant>,
    high_checks: u32,
}

static TRACK: Mutex<Option<Track>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Track) -> R) -> R {
    f(TRACK.lock().unwrap().get_or_insert_with(|| Track {
        started: Instant::now(),
        last_active: HashMap::new(),
        last_run: None,
        last_auto: None,
        high_checks: 0,
    }))
}

pub fn key(exe: &str) -> String {
    exe.to_lowercase().trim_end_matches(".exe").to_string()
}

/// Chamado pelos sensores com o app em uso.
pub fn touch(exe: &str) {
    let k = key(exe);
    with(|t| {
        t.last_active.insert(k, Instant::now());
    });
}

fn protected(name: &str) -> bool {
    PROTECTED.contains(&name) || name.starts_with("pipo")
}

/// Há quanto tempo o app não é usado (None = nunca visto em uso desde que o Pipo abriu).
fn idle_for(name: &str) -> (Option<Duration>, Duration) {
    with(|t| (t.last_active.get(name).map(|i| i.elapsed()), t.started.elapsed()))
}

#[derive(Clone)]
struct Proc {
    pid: u32,
    name: String,
    memory: u64,
}

fn processes() -> (Vec<Proc>, u64, u64) {
    let Ok(mut sys) = System::new() else { return (Vec::new(), 0, 0) };
    sys.refresh_memory();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_memory());
    let list = sys
        .processes()
        .values()
        .map(|p| Proc { pid: p.pid().as_u32(), name: key(&p.name().to_string_lossy()), memory: p.memory() })
        .collect();
    (list, sys.used_memory(), sys.total_memory())
}

/// O app em primeiro plano agora (nunca é mexido).
fn foreground(app: &AppHandle) -> Option<String> {
    app.state::<AppState>().sense.lock().unwrap().last_app.as_ref().map(|a| key(&a.exe))
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppMemory {
    name: String,
    memory: u64,
    processes: usize,
    /// Minutos sem uso (None = não foi usado desde que o Pipo abriu).
    idle_min: Option<u64>,
    since_start_min: u64,
    /// Pesado e parado há bastante tempo: dá para sugerir fechar.
    suggest_close: bool,
    protected: bool,
}

/// Agrupa por app e ordena do que mais usa para o que menos usa.
fn group(list: &[Proc], foreground: Option<&str>) -> Vec<AppMemory> {
    let mut by: HashMap<String, (u64, usize)> = HashMap::new();
    for p in list {
        let e = by.entry(p.name.clone()).or_default();
        e.0 += p.memory;
        e.1 += 1;
    }
    let mut out: Vec<AppMemory> = by
        .into_iter()
        .map(|(name, (memory, processes))| {
            let (idle, since) = idle_for(&name);
            let is_protected = protected(&name) || Some(name.as_str()) == foreground;
            let quiet = idle.unwrap_or(since);
            AppMemory {
                suggest_close: !is_protected && memory >= HEAVY && quiet >= CLOSE_AFTER,
                idle_min: idle.map(|d| d.as_secs() / 60),
                since_start_min: since.as_secs() / 60,
                protected: is_protected,
                name,
                memory,
                processes,
            }
        })
        .collect();
    out.sort_by(|a, b| b.memory.cmp(&a.memory));
    out
}

#[cfg(windows)]
fn trim(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::EmptyWorkingSet;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = EmptyWorkingSet(h) != 0;
        CloseHandle(h);
        ok
    }
}

#[cfg(not(windows))]
fn trim(_pid: u32) -> bool {
    false
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeResult {
    /// Bytes devolvidos ao Windows.
    freed: u64,
    apps: usize,
    used_before: u64,
    used_after: u64,
}

/// Devolve ao Windows a memória dos apps parados (bloqueante: ~1,5 s).
pub fn optimize(app: &AppHandle) -> OptimizeResult {
    let fg = foreground(app);
    let gaming = app.state::<AppState>().sense.lock().unwrap().gaming;
    let (list, used_before, _) = processes();
    let own = std::process::id();
    let mut touched: Vec<u32> = Vec::new();
    let mut names = std::collections::HashSet::new();
    for p in &list {
        if protected(&p.name) || Some(p.name.as_str()) == fg.as_deref() || p.memory < 30 * 1024 * 1024 {
            continue;
        }
        // Jogando: só o que está claramente parado (o jogo é o app em primeiro plano).
        let (idle, since) = idle_for(&p.name);
        let quiet = idle.unwrap_or(since);
        if quiet < RECENT && !(gaming && idle.is_none()) {
            continue;
        }
        if trim(p.pid) {
            touched.push(p.pid);
            names.insert(p.name.clone());
        }
    }
    // O próprio Pipo também devolve o que não está usando.
    if trim(own) {
        touched.push(own);
    }
    std::thread::sleep(Duration::from_millis(1200));
    let (_, used_after, _) = processes();
    let freed = used_before.saturating_sub(used_after);
    with(|t| t.last_run = Some((chrono::Local::now(), freed, names.len())));
    OptimizeResult { freed, apps: names.len(), used_before, used_after }
}

/// Fecha o app de um jeito gentil (como clicar no X das janelas): ele pode pedir para salvar.
#[cfg(windows)]
fn close_windows(pids: &[u32]) -> usize {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindow, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, GW_OWNER, WM_CLOSE};
    struct Ctx<'a> {
        pids: &'a [u32],
        closed: usize,
    }
    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> i32 {
        unsafe {
            let ctx = &mut *(lparam as *mut Ctx);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if ctx.pids.contains(&pid) && IsWindowVisible(hwnd) != 0 && GetWindow(hwnd, GW_OWNER).is_null() && PostMessageW(hwnd, WM_CLOSE, 0, 0) != 0 {
                ctx.closed += 1;
            }
        }
        1
    }
    let mut ctx = Ctx { pids, closed: 0 };
    unsafe { EnumWindows(Some(each), &mut ctx as *mut Ctx as LPARAM) };
    ctx.closed
}

#[cfg(not(windows))]
fn close_windows(_pids: &[u32]) -> usize {
    0
}

fn size(bytes: u64) -> String {
    crate::guardian::size_text(bytes)
}

/// Texto curto do resultado (para o chat, o Telegram e o aviso).
pub fn result_text(r: &OptimizeResult) -> String {
    if r.freed < 50 * 1024 * 1024 {
        "A memória já estava em ordem: não havia quase nada parado para liberar.".into()
    } else {
        format!("Liberei {} de {} app(s) parado(s).", size(r.freed), r.apps)
    }
}

/// Resumo para o chat: uso agora, os maiores e o que dá para fechar.
pub fn summary_text(app: &AppHandle) -> String {
    let fg = foreground(app);
    let (list, used, total) = processes();
    let apps = group(&list, fg.as_deref());
    let mut out = vec![format!("Memória em uso: {} de {} ({:.0}%).", size(used), size(total), used as f64 * 100.0 / total.max(1) as f64)];
    for a in apps.iter().filter(|a| !a.protected).take(6) {
        let idle = match a.idle_min {
            Some(0) => "em uso agora".to_string(),
            Some(m) => format!("sem uso há {m} min"),
            None => "sem uso desde que o Pipo abriu".to_string(),
        };
        out.push(format!("- {}: {} ({idle}){}", a.name, size(a.memory), if a.suggest_close { " — dá para fechar" } else { "" }));
    }
    out.join("\n")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStatus {
    used: u64,
    total: u64,
    apps: Vec<AppMemory>,
    last_run: Option<String>,
    last_freed: u64,
}

#[tauri::command]
pub async fn memory_status(app: AppHandle) -> MemoryStatus {
    let handle = app.clone();
    let (list, used, total) = tauri::async_runtime::spawn_blocking(processes).await.unwrap_or_default();
    let fg = foreground(&handle);
    let apps: Vec<AppMemory> = group(&list, fg.as_deref()).into_iter().filter(|a| !a.protected).take(8).collect();
    let (last_run, last_freed) = with(|t| t.last_run.map(|(at, freed, _)| (Some(at.format("%H:%M").to_string()), freed))).unwrap_or((None, 0));
    MemoryStatus { used, total, apps, last_run, last_freed }
}

#[tauri::command]
pub async fn memory_optimize(app: AppHandle) -> Result<OptimizeResult, String> {
    let handle = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || optimize(&handle)).await.map_err(|e| e.to_string())?;
    crate::journal::add(&app, "aviso", "Memória otimizada", &result_text(&r), "info");
    Ok(r)
}

/// Fecha (com gentileza) todas as janelas de um app.
#[tauri::command]
pub fn memory_close_app(app: AppHandle, name: String) -> Result<usize, String> {
    let name = key(&name);
    if protected(&name) || foreground(&app).as_deref() == Some(name.as_str()) {
        return Err("Esse app o Pipo não fecha (é do sistema ou está em uso agora).".into());
    }
    let (list, _, _) = processes();
    let pids: Vec<u32> = list.iter().filter(|p| p.name == name).map(|p| p.pid).collect();
    let closed = close_windows(&pids);
    if closed == 0 {
        return Err("Esse app não tem janela aberta para fechar (ele roda em segundo plano).".into());
    }
    Ok(closed)
}

/// Porcentagem da memória em uso agora (leitura barata do Windows).
#[cfg(windows)]
fn load_percent() -> u32 {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut s: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    s.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut s) } != 0 {
        s.dwMemoryLoad
    } else {
        0
    }
}

#[cfg(not(windows))]
fn load_percent() -> u32 {
    0
}

/// Guardião: com o modo automático ligado, otimiza quando a memória passa do limite
/// por ~1 min seguido (no máximo a cada 10 min). Chamado a cada 2 s.
pub fn tick(app: &AppHandle) {
    let (auto, limit) = {
        let state = app.state::<AppState>();
        let s = state.settings.lock().unwrap();
        (s.memory_auto, s.memory_auto_percent)
    };
    if !auto {
        return;
    }
    let percent = load_percent();
    let due = with(|t| {
        if percent >= limit as u32 {
            t.high_checks += 1;
        } else {
            t.high_checks = 0;
        }
        let ready = t.high_checks >= 30 && t.last_auto.is_none_or(|i| i.elapsed() >= AUTO_EVERY);
        if ready {
            t.high_checks = 0;
            t.last_auto = Some(Instant::now());
        }
        ready
    });
    if !due {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let r = optimize(&app);
        crate::journal::add(&app, "aviso", "Memória otimizada sozinha", &result_text(&r), "info");
        if r.freed >= 200 * 1024 * 1024 {
            crate::toast::notify_from(&app, "guardian", "info", "Memória aliviada", &result_text(&r), Some("view:system"));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agrupa_e_protege() {
        let list = vec![
            Proc { pid: 1, name: "chrome".into(), memory: 400 << 20 },
            Proc { pid: 2, name: "chrome".into(), memory: 300 << 20 },
            Proc { pid: 3, name: "svchost".into(), memory: 900 << 20 },
            Proc { pid: 4, name: "code".into(), memory: 500 << 20 },
        ];
        touch("Code.exe");
        let apps = group(&list, Some("code"));
        assert_eq!(apps[0].name, "svchost");
        assert!(apps[0].protected);
        let chrome = apps.iter().find(|a| a.name == "chrome").unwrap();
        assert_eq!((chrome.memory, chrome.processes), (700 << 20, 2));
        let code = apps.iter().find(|a| a.name == "code").unwrap();
        assert!(code.protected && !code.suggest_close, "o app em uso nunca é mexido");
        assert_eq!(key("Discord.EXE"), "discord");
    }

    #[test]
    #[ignore = "mexe na memória dos apps parados deste PC (só devolve ao Windows; nada é fechado)"]
    fn otimiza_de_verdade() {
        let (list, used, total) = processes();
        println!("em uso {} de {} · {} processos", size(used), size(total), list.len());
        for a in group(&list, None).iter().filter(|a| !a.protected).take(5) {
            println!("  {} {} ({} proc.)", a.name, size(a.memory), a.processes);
        }
    }
}

#[cfg(all(test, windows))]
mod live_trim {
    /// Abre um processo de teste que segura ~300 MB, alivia só ele e mede.
    #[test]
    #[ignore = "abre um processo de teste (python)"]
    fn alivia_um_processo_de_teste() {
        let mut child = std::process::Command::new("python")
            .args(["-I", "-c", "import time; b = bytearray(300*1024*1024)\nfor i in range(0, len(b), 4096): b[i] = 1\ntime.sleep(20)"])
            .spawn()
            .expect("python");
        std::thread::sleep(std::time::Duration::from_secs(4));
        let pid = child.id();
        let mem = |pid: u32| super::processes().0.into_iter().find(|p| p.pid == pid).map(|p| p.memory).unwrap_or(0);
        let before = mem(pid);
        assert!(super::trim(pid));
        std::thread::sleep(std::time::Duration::from_millis(800));
        let after = mem(pid);
        println!("antes {} MB, depois {} MB", before >> 20, after >> 20);
        let _ = child.kill();
        assert!(before > 250 << 20 && after < 50 << 20);
    }
}

//! Fase 9.5: guardião do PC. Avisa quando um disco está quase cheio (com o que dá para
//! liberar), quando o app em uso trava, e cuida do Windows Update na hora do jogo: avisa
//! antes de jogar se o Windows está esperando reiniciar, conta depois da partida se ele
//! ficou trabalhando no meio, e confere o Modo de Jogo do Windows (que é o que segura as
//! atualizações e os pedidos de reinício enquanto um jogo está aberto). Também roda a
//! organização automática da pasta Downloads, uma vez por dia, se a pessoa ligou.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{Local, NaiveDate};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::AppState;

const DISK_EVERY: Duration = Duration::from_secs(10 * 60);
/// O app precisa ficar travado por este tempo para virar aviso.
const HUNG_AFTER: Duration = Duration::from_secs(15);
/// Processos do Windows Update (o que aparece trabalhando durante uma atualização).
const UPDATE_PROCESSES: &[&str] = &["tiworker", "mousocoreworker", "usocoreworker", "wuauclt", "trustedinstaller", "musnotification"];
pub const UPDATE_SETTINGS: &str = "ms-settings:windowsupdate";

#[derive(Default)]
struct Guard {
    last_disk: Option<Instant>,
    /// Por disco: dia e nível (1 = pouco espaço, 2 = quase nada) do último aviso.
    disk_warned: Vec<(String, NaiveDate, u8)>,
    hung: Option<(String, Instant)>,
    hung_warned: Option<String>,
    was_gaming: bool,
    reboot_warned: Option<NaiveDate>,
    /// Pico de CPU do Windows Update durante o jogo atual.
    update_peak: f32,
    update_samples: u32,
    last_update_check: Option<Instant>,
    auto_organized: Option<NaiveDate>,
}

static GUARD: Mutex<Option<Guard>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Guard) -> R) -> R {
    f(GUARD.lock().unwrap().get_or_insert_with(Guard::default))
}

fn settings(app: &AppHandle) -> (bool, bool) {
    let state = app.state::<AppState>();
    let s = state.settings.lock().unwrap();
    (s.guardian, s.downloads_auto)
}

/// "4,2 GB", "820 MB".
pub fn size_text(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1e9).replace('.', ",")
    } else {
        format!("{} MB", (bytes / 1_000_000).max(1))
    }
}

/// Nível de alerta de um disco: 2 = quase nada (menos de 2 GB, ou de 5 GB e 3%), 1 = pouco (menos de 10 GB e 10%).
pub fn disk_level(free: u64, total: u64) -> u8 {
    let pct = if total == 0 { 1.0 } else { free as f64 / total as f64 };
    if free < 2_000_000_000 || (free < 5_000_000_000 && pct < 0.03) {
        2
    } else if free < 10_000_000_000 && pct < 0.10 {
        1
    } else {
        0
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Drive {
    letter: String,
    free: u64,
    total: u64,
    level: u8,
}

/// Discos fixos (HDs e SSDs) com espaço livre e total.
#[cfg(windows)]
pub fn drives() -> Vec<Drive> {
    use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
    const DRIVE_FIXED: u32 = 3;
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = format!("{}:", (b'A' + i as u8) as char);
        let root: Vec<u16> = format!("{letter}\\").encode_utf16().chain([0]).collect();
        if unsafe { GetDriveTypeW(root.as_ptr()) } != DRIVE_FIXED {
            continue;
        }
        let (mut free, mut total) = (0u64, 0u64);
        if unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &mut free, &mut total, std::ptr::null_mut()) } == 0 || total == 0 {
            continue;
        }
        out.push(Drive { level: disk_level(free, total), letter, free, total });
    }
    out
}

#[cfg(not(windows))]
pub fn drives() -> Vec<Drive> {
    Vec::new()
}

/// Bytes na Lixeira (de todos os discos).
#[cfg(windows)]
fn recycle_bin_bytes() -> u64 {
    use windows_sys::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info: SHQUERYRBINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHQUERYRBINFO>() as u32;
    if unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) } == 0 {
        info.i64Size.max(0) as u64
    } else {
        0
    }
}

#[cfg(not(windows))]
fn recycle_bin_bytes() -> u64 {
    0
}

fn check_disks(app: &AppHandle) {
    let due = with(|g| {
        let due = g.last_disk.is_none_or(|t| t.elapsed() >= DISK_EVERY);
        if due {
            g.last_disk = Some(Instant::now());
        }
        due
    });
    if !due {
        return;
    }
    let today = Local::now().date_naive();
    for drive in drives() {
        if drive.level == 0 {
            continue;
        }
        // Um aviso por dia por disco, ou de novo se piorou.
        let warn = with(|g| {
            let previous = g.disk_warned.iter().find(|(l, _, _)| *l == drive.letter).cloned();
            let warn = match previous {
                Some((_, day, level)) => day != today || drive.level > level,
                None => true,
            };
            if warn {
                g.disk_warned.retain(|(l, _, _)| *l != drive.letter);
                g.disk_warned.push((drive.letter.clone(), today, drive.level));
            }
            warn
        });
        if !warn {
            continue;
        }
        let mut hints = Vec::new();
        let bin = recycle_bin_bytes();
        if bin >= 500_000_000 {
            hints.push(format!("Lixeira: {}", size_text(bin)));
        }
        if drive.letter == "C:" {
            let downloads = crate::actions::downloads::total_bytes();
            if downloads >= 1_000_000_000 {
                hints.push(format!("Downloads: {}", size_text(downloads)));
            }
        }
        let title = if drive.level == 2 {
            format!("Disco {} quase sem espaço", drive.letter)
        } else {
            format!("Disco {} ficando cheio", drive.letter)
        };
        let body = if hints.is_empty() {
            format!("{} livres de {}.", size_text(drive.free), size_text(drive.total))
        } else {
            format!("{} livres. Dá para liberar: {}.", size_text(drive.free), hints.join(" · "))
        };
        crate::windows::set_pet_state(app, "alert");
        crate::toast::notify_from(app, "guardian", "alert", &title, &body, Some("view:actions"));
    }
}

/// O app em primeiro plano parou de responder? Devolve o nome para o aviso.
#[cfg(windows)]
fn foreground_hung() -> Option<String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowTextW, IsHungAppWindow};
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return None;
    }
    let mut class = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    let class = String::from_utf16_lossy(&class[..n.max(0) as usize]);
    let mut title = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
    let title = String::from_utf16_lossy(&title[..n.max(0) as usize]);
    // Janela "fantasma": o Windows põe no lugar da janela travada, com "(Não está respondendo)".
    let hung = unsafe { IsHungAppWindow(hwnd) } != 0 || class == "Ghost";
    hung.then(|| hung_name(&title)).flatten()
}

#[cfg(not(windows))]
fn foreground_hung() -> Option<String> {
    None
}

/// "Documento - Word (Não está respondendo)" -> "Word".
pub fn hung_name(title: &str) -> Option<String> {
    let mut t = title.trim().to_string();
    for suffix in ["(Não está respondendo)", "(Não Respondendo)", "(Not Responding)"] {
        if let Some(rest) = t.strip_suffix(suffix) {
            t = rest.trim().to_string();
        }
    }
    let name = t.rsplit(" - ").next().unwrap_or(&t).trim().to_string();
    (!name.is_empty()).then_some(name)
}

fn check_hung(app: &AppHandle) {
    let now = foreground_hung();
    let warn = with(|g| match now {
        None => {
            g.hung = None;
            g.hung_warned = None;
            None
        }
        Some(name) => {
            let since = match &g.hung {
                Some((n, t)) if *n == name => *t,
                _ => {
                    g.hung = Some((name.clone(), Instant::now()));
                    Instant::now()
                }
            };
            if since.elapsed() >= HUNG_AFTER && g.hung_warned.as_deref() != Some(&name) {
                g.hung_warned = Some(name.clone());
                Some(name)
            } else {
                None
            }
        }
    });
    if let Some(name) = warn {
        crate::windows::set_pet_state(app, "alert");
        let prompt = format!("prompt:O {name} travou (parou de responder). Fecha ele para mim.");
        crate::toast::notify_from(
            app,
            "guardian",
            "alert",
            &format!("{name} parou de responder"),
            "Se não voltar, clique aqui que eu fecho para você.",
            Some(&prompt),
        );
    }
}

/// O Windows está esperando reiniciar para terminar uma atualização?
#[cfg(windows)]
pub fn reboot_pending() -> bool {
    use windows_sys::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ};
    let paths = [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending",
    ];
    paths.iter().any(|p| {
        let wide: Vec<u16> = p.encode_utf16().chain([0]).collect();
        let mut key: HKEY = std::ptr::null_mut();
        let ok = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, wide.as_ptr(), 0, KEY_READ, &mut key) } == 0;
        if ok {
            unsafe { RegCloseKey(key) };
        }
        ok
    })
}

#[cfg(not(windows))]
pub fn reboot_pending() -> bool {
    false
}

const GAME_BAR: &str = r"Software\Microsoft\GameBar";

/// O Modo de Jogo do Windows está ligado? (sem o valor no registro, o padrão do Windows 11 é ligado)
#[cfg(windows)]
pub fn windows_game_mode() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let key: Vec<u16> = GAME_BAR.encode_utf16().chain([0]).collect();
    let name: Vec<u16> = "AutoGameModeEnabled".encode_utf16().chain([0]).collect();
    let mut value: u32 = 1;
    let mut size = 4u32;
    let code = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), (&mut value as *mut u32).cast(), &mut size) };
    code != 0 || value != 0
}

#[cfg(not(windows))]
pub fn windows_game_mode() -> bool {
    true
}

fn check_game_updates(app: &AppHandle, gaming: bool) {
    let today = Local::now().date_naive();
    let (started, ended, peak) = with(|g| {
        let started = gaming && !g.was_gaming;
        let ended = !gaming && g.was_gaming;
        g.was_gaming = gaming;
        let peak = if ended { std::mem::take(&mut g.update_peak) } else { 0.0 };
        if ended || started {
            g.update_samples = 0;
        }
        (started, ended, peak)
    });
    // Antes da partida: atualização esperando reiniciar (o Windows pode pedir no meio).
    if started && with(|g| g.reboot_warned != Some(today)) && reboot_pending() {
        with(|g| g.reboot_warned = Some(today));
        crate::toast::notify_from(
            app,
            "guardian",
            "alert",
            "Windows esperando reiniciar",
            "Tem atualização pendente. Reinicie depois do jogo para ela não atrapalhar outra partida.",
            None,
        );
    }
    // Durante o jogo: anota quanto o Windows Update usou (sem aviso na tela no meio da partida).
    if gaming && with(|g| g.last_update_check.is_none_or(|t| t.elapsed() >= Duration::from_secs(30))) {
        with(|g| g.last_update_check = Some(Instant::now()));
        let busy: f32 = app
            .state::<AppState>()
            .hardware
            .lock()
            .unwrap()
            .processes()
            .iter()
            .filter(|p| UPDATE_PROCESSES.contains(&p.name.to_lowercase().as_str()))
            .map(|p| p.cpu)
            .sum();
        with(|g| {
            if busy >= 5.0 {
                g.update_samples += 1;
                g.update_peak = g.update_peak.max(busy);
            }
        });
    }
    // Depois do jogo: conta, e oferece pausar as atualizações (na tela do Windows Update).
    if ended && peak >= 5.0 {
        let tip = if windows_game_mode() { "" } else { " O Modo de Jogo do Windows está desligado: ligue em ⚙ → Sensores." };
        crate::toast::notify_from(
            app,
            "guardian",
            "info",
            "O Windows Update trabalhou durante o jogo",
            &format!("Chegou a {peak:.0}% da CPU. Clique para pausar as atualizações por uma semana.{tip}"),
            Some(UPDATE_SETTINGS),
        );
    }
}

fn auto_organize(app: &AppHandle) {
    let today = Local::now().date_naive();
    if with(|g| g.auto_organized == Some(today)) {
        return;
    }
    with(|g| g.auto_organized = Some(today));
    // Só o que chegou antes de ontem: o que é de hoje ainda pode estar em uso.
    if let Ok(n) = crate::actions::downloads::organize_older(app, Duration::from_secs(24 * 3600)) {
        if n > 0 {
            crate::toast::notify_from(
                app,
                "guardian",
                "done",
                &format!("Organizei {n} arquivo(s) de Downloads"),
                "Separados em pastas por tipo. Dá para desfazer em Ações → Downloads.",
                Some("view:actions"),
            );
        }
    }
}

/// Chamado pelos sensores a cada 2 s.
pub fn tick(app: &AppHandle, gaming: bool, idle: u64) {
    let (on, auto) = settings(app);
    if on {
        if !gaming {
            check_disks(app);
            check_hung(app);
        }
        check_game_updates(app, gaming);
    }
    // Organiza com o PC parado há 2 min (para não mexer nos arquivos enquanto a pessoa usa).
    if auto && !gaming && idle >= 120 {
        auto_organize(app);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardianStatus {
    drives: Vec<Drive>,
    game_mode: bool,
    reboot_pending: bool,
}

#[tauri::command]
pub fn guardian_status() -> GuardianStatus {
    GuardianStatus { drives: drives(), game_mode: windows_game_mode(), reboot_pending: reboot_pending() }
}

/// Liga o Modo de Jogo do Windows (configuração do próprio usuário, sem pedir administrador).
#[tauri::command]
pub fn guardian_enable_game_mode() -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD};
        let key: Vec<u16> = GAME_BAR.encode_utf16().chain([0]).collect();
        let name: Vec<u16> = "AutoGameModeEnabled".encode_utf16().chain([0]).collect();
        let value: u32 = 1;
        let code = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_DWORD, (&value as *const u32).cast(), 4) };
        if code != 0 {
            return Err(format!("O Windows não deixou ligar (código {code}). Ligue em Configurações → Jogos → Modo de Jogo."));
        }
    }
    Ok(())
}

/// Abre a tela do Windows Update (lá dá para pausar as atualizações).
#[tauri::command]
pub fn guardian_open_updates() -> Result<(), String> {
    crate::actions::shell_open(UPDATE_SETTINGS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nivel_de_disco() {
        const GB: u64 = 1_000_000_000;
        assert_eq!(disk_level(200 * GB, 500 * GB), 0);
        assert_eq!(disk_level(30 * GB, 1000 * GB), 0); // 3%, mas 30 GB ainda é bastante
        assert_eq!(disk_level(8 * GB, 500 * GB), 1);
        assert_eq!(disk_level(GB, 500 * GB), 2);
        assert_eq!(disk_level(9 * GB, 60 * GB), 0); // 15% de um disco pequeno
        assert_eq!(size_text(4_200_000_000), "4,2 GB");
        assert_eq!(size_text(820_000_000), "820 MB");
    }

    #[test]
    fn nome_do_app_travado() {
        assert_eq!(hung_name("Relatório.docx - Word (Não está respondendo)").as_deref(), Some("Word"));
        assert_eq!(hung_name("Spotify (Not Responding)").as_deref(), Some("Spotify"));
        assert_eq!(hung_name(""), None);
    }

    #[test]
    #[ignore = "lê os discos e o registro deste PC"]
    fn mostra_o_estado() {
        for d in drives() {
            println!("{} livre {} de {} nível {}", d.letter, size_text(d.free), size_text(d.total), d.level);
        }
        println!("modo de jogo {} · reinício pendente {}", windows_game_mode(), reboot_pending());
    }
}

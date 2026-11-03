//! O que está acontecendo no computador agora: app em primeiro plano, tela cheia
//! e tempo sem uso. Tudo é lido localmente e nada sai do computador.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Dev,
    Docs,
    Browser,
    Live,
    Games,
    Chat,
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveApp {
    pub exe: String,
    pub name: String,
    pub title: String,
    pub category: Category,
    pub project: Option<String>,
    pub fullscreen: bool,
}

/// Nome amigável e categoria a partir do executável.
fn classify(exe: &str, title: &str) -> (String, Category) {
    let known: &[(&str, &str, Category)] = &[
        ("code.exe", "VS Code", Category::Dev),
        ("code - insiders.exe", "VS Code Insiders", Category::Dev),
        ("cursor.exe", "Cursor", Category::Dev),
        ("devenv.exe", "Visual Studio", Category::Dev),
        ("idea64.exe", "IntelliJ IDEA", Category::Dev),
        ("rider64.exe", "Rider", Category::Dev),
        ("pycharm64.exe", "PyCharm", Category::Dev),
        ("webstorm64.exe", "WebStorm", Category::Dev),
        ("studio64.exe", "Android Studio", Category::Dev),
        ("sublime_text.exe", "Sublime Text", Category::Dev),
        ("windowsterminal.exe", "Terminal", Category::Dev),
        ("wt.exe", "Terminal", Category::Dev),
        ("cmd.exe", "Prompt de Comando", Category::Dev),
        ("powershell.exe", "PowerShell", Category::Dev),
        ("pwsh.exe", "PowerShell", Category::Dev),
        ("githubdesktop.exe", "GitHub Desktop", Category::Dev),
        ("docker desktop.exe", "Docker Desktop", Category::Dev),
        ("winword.exe", "Word", Category::Docs),
        ("excel.exe", "Excel", Category::Docs),
        ("powerpnt.exe", "PowerPoint", Category::Docs),
        ("onenote.exe", "OneNote", Category::Docs),
        ("acrobat.exe", "Acrobat", Category::Docs),
        ("acrord32.exe", "Acrobat Reader", Category::Docs),
        ("notion.exe", "Notion", Category::Docs),
        ("obsidian.exe", "Obsidian", Category::Docs),
        ("chrome.exe", "Chrome", Category::Browser),
        ("msedge.exe", "Edge", Category::Browser),
        ("firefox.exe", "Firefox", Category::Browser),
        ("brave.exe", "Brave", Category::Browser),
        ("opera.exe", "Opera", Category::Browser),
        ("opera_gx.exe", "Opera GX", Category::Browser),
        ("vivaldi.exe", "Vivaldi", Category::Browser),
        ("steam.exe", "Steam", Category::Games),
        ("steamwebhelper.exe", "Steam", Category::Games),
        ("epicgameslauncher.exe", "Epic Games", Category::Games),
        ("battle.net.exe", "Battle.net", Category::Games),
        ("riotclientservices.exe", "Riot Client", Category::Games),
        ("discord.exe", "Discord", Category::Chat),
        ("slack.exe", "Slack", Category::Chat),
        ("ms-teams.exe", "Teams", Category::Chat),
        ("whatsapp.exe", "WhatsApp", Category::Chat),
    ];
    let lower = exe.to_lowercase();
    let (name, mut category) = known
        .iter()
        .find(|(e, _, _)| *e == lower)
        .map(|(_, n, c)| (n.to_string(), *c))
        .unwrap_or_else(|| (exe.trim_end_matches(".exe").to_string(), Category::Other));

    let t = title.to_lowercase();
    if category == Category::Browser
        && (t.contains("twitch") || t.contains("kick.com") || (t.contains("youtube") && (t.contains("ao vivo") || t.contains("live"))))
    {
        category = Category::Live;
    }
    (name, category)
}

/// Nome do projeto a partir do título da janela de editores e terminais.
fn project_from_title(category: Category, name: &str, title: &str) -> Option<String> {
    if category != Category::Dev {
        return None;
    }
    let title = title.trim_start_matches("● ").trim();
    let parts: Vec<&str> = title.split(" - ").map(str::trim).collect();
    let project = if name.starts_with("VS Code") || name == "Cursor" {
        // "arquivo.ts - projeto - Visual Studio Code"
        if parts.len() >= 3 { parts[parts.len() - 2] } else { parts[0] }
    } else if title.contains(" – ") {
        // JetBrains: "projeto – arquivo"
        title.split(" – ").next().unwrap_or("")
    } else if title.contains('\\') || title.contains('/') {
        // Terminais costumam mostrar a pasta atual
        title.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or("")
    } else {
        ""
    };
    let project = project.trim();
    (!project.is_empty() && project.len() < 80).then(|| project.to_string())
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows_sys::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetDesktopWindow, GetForegroundWindow, GetShellWindow, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
    };

    pub struct Foreground {
        pub pid: u32,
        pub exe: String,
        pub title: String,
        pub fullscreen: bool,
    }

    fn exe_name(pid: u32) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }
            let mut buf = [0u16; 512];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(handle);
            if ok == 0 {
                return None;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            path.rsplit('\\').next().map(String::from)
        }
    }

    /// A janela ocupa o monitor inteiro (inclusive a área da barra de tarefas).
    fn covers_monitor(hwnd: HWND) -> bool {
        unsafe {
            if hwnd == GetShellWindow() || hwnd == GetDesktopWindow() {
                return false;
            }
            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return false;
            }
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                return false;
            }
            let m = info.rcMonitor;
            rect.left <= m.left && rect.top <= m.top && rect.right >= m.right && rect.bottom >= m.bottom
        }
    }

    fn exclusive_fullscreen() -> bool {
        let mut state = 0;
        unsafe { SHQueryUserNotificationState(&mut state) == 0 && state == QUNS_RUNNING_D3D_FULL_SCREEN }
    }

    pub fn foreground() -> Option<Foreground> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let mut buf = [0u16; 512];
            let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            let title = String::from_utf16_lossy(&buf[..len.max(0) as usize]);
            let exe = exe_name(pid).unwrap_or_default();
            let fullscreen = exe.to_lowercase() != "explorer.exe" && (covers_monitor(hwnd) || exclusive_fullscreen());
            Some(Foreground { pid, exe, title, fullscreen })
        }
    }

    pub fn idle_seconds() -> u64 {
        unsafe {
            let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
            if GetLastInputInfo(&mut info) == 0 {
                return 0;
            }
            (GetTickCount().wrapping_sub(info.dwTime) / 1000) as u64
        }
    }
}

/// App em primeiro plano, ignorando o próprio Pipo.
#[cfg(windows)]
pub fn active_app(own_pid: u32) -> Option<ActiveApp> {
    let fg = win::foreground()?;
    if fg.pid == own_pid || fg.exe.is_empty() {
        return None;
    }
    let (name, category) = classify(&fg.exe, &fg.title);
    let project = project_from_title(category, &name, &fg.title);
    Some(ActiveApp { exe: fg.exe, name, title: fg.title, category, project, fullscreen: fg.fullscreen })
}

/// O próprio Pipo (painel ou personagem) está em primeiro plano.
#[cfg(windows)]
pub fn own_foreground(own_pid: u32) -> bool {
    win::foreground().is_some_and(|fg| fg.pid == own_pid)
}

#[cfg(windows)]
pub fn idle_seconds() -> u64 {
    win::idle_seconds()
}

#[cfg(not(windows))]
pub fn active_app(_own_pid: u32) -> Option<ActiveApp> {
    None
}

#[cfg(not(windows))]
pub fn own_foreground(_own_pid: u32) -> bool {
    false
}

#[cfg(not(windows))]
pub fn idle_seconds() -> u64 {
    0
}

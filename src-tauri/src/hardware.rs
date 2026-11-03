//! Uso de CPU, memória e GPU (NVIDIA) lido localmente.

use std::collections::HashMap;
use std::time::Duration;

use serde::Serialize;
use sysinfo::{ProcessesToUpdate, System};
use tauri::{AppHandle, Manager};

use crate::AppState;

pub struct Hardware {
    /// `None` se o sistema não puder ser lido; os números então ficam zerados.
    sys: Option<System>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    name: String,
    usage: u32,
    mem_used: u64,
    mem_total: u64,
    temperature: Option<u32>,
}

/// Lê a GPU NVIDIA pelo `nvidia-smi` que vem com o driver. Carregar a biblioteca
/// da NVIDIA dentro do Pipo deixaria ~25 MB presos na memória.
fn nvidia_gpu() -> Option<GpuInfo> {
    let mut command = std::process::Command::new("nvidia-smi");
    command.args([
        "--query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu",
        "--format=csv,noheader,nounits",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok().filter(|o| o.status.success())?;
    let line = String::from_utf8_lossy(&output.stdout).lines().next()?.to_string();
    let fields: Vec<&str> = line.split(',').map(str::trim).collect();
    let [name, usage, used, total, temperature] = fields.as_slice() else { return None };
    const MIB: u64 = 1024 * 1024;
    Some(GpuInfo {
        name: name.to_string(),
        usage: usage.parse().unwrap_or(0),
        mem_used: used.parse::<u64>().unwrap_or(0) * MIB,
        mem_total: total.parse::<u64>().unwrap_or(0) * MIB,
        temperature: temperature.parse().ok(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessUsage {
    pub name: String,
    pub cpu: f32,
    pub memory: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    cpu: f32,
    cores: usize,
    mem_used: u64,
    mem_total: u64,
    gpu: Option<GpuInfo>,
    top_cpu: Vec<ProcessUsage>,
    top_memory: Vec<ProcessUsage>,
}

impl Hardware {
    pub fn new() -> Self {
        Self { sys: System::new().ok() }
    }

    /// Descarta a lista de processos quando a aba Sistema fecha.
    pub fn release(&mut self) {
        self.sys = System::new().ok();
    }

    /// Amostra leve para os alertas: (CPU em %, fração da memória em uso).
    pub fn sample(&mut self) -> (f32, f64) {
        let Some(sys) = self.sys.as_mut() else { return (0.0, 0.0) };
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let total = sys.total_memory().max(1);
        (sys.global_cpu_usage(), sys.used_memory() as f64 / total as f64)
    }

    /// Processos agrupados por nome (ex.: todas as abas do Chrome somadas).
    pub fn processes(&mut self) -> Vec<ProcessUsage> {
        let Some(sys) = self.sys.as_mut() else { return Vec::new() };
        sys.refresh_processes(ProcessesToUpdate::All, true);
        std::thread::sleep(Duration::from_millis(300));
        sys.refresh_processes(ProcessesToUpdate::All, true);
        let cores = sys.cpus().len().max(1) as f32;
        let mut grouped: HashMap<String, ProcessUsage> = HashMap::new();
        for process in sys.processes().values() {
            let name = process.name().to_string_lossy().trim_end_matches(".exe").to_string();
            let entry = grouped
                .entry(name.clone())
                .or_insert(ProcessUsage { name, cpu: 0.0, memory: 0 });
            entry.cpu += process.cpu_usage() / cores;
            entry.memory += process.memory();
        }
        grouped.into_values().collect()
    }

    pub fn snapshot(&mut self) -> HardwareInfo {
        let (cpu, _) = self.sample();
        let mut processes = self.processes();
        processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
        let top_cpu: Vec<ProcessUsage> = processes
            .iter()
            .take(5)
            .map(|p| ProcessUsage { name: p.name.clone(), cpu: p.cpu, memory: p.memory })
            .collect();
        processes.sort_by(|a, b| b.memory.cmp(&a.memory));
        let top_memory = processes.into_iter().take(5).collect();
        let gpu = nvidia_gpu();
        let sys = self.sys.as_ref();
        HardwareInfo {
            cpu,
            cores: sys.map_or(0, |s| s.cpus().len()),
            mem_used: sys.map_or(0, |s| s.used_memory()),
            mem_total: sys.map_or(0, |s| s.total_memory()),
            gpu,
            top_cpu,
            top_memory,
        }
    }
}

#[tauri::command]
pub fn system_release(app: AppHandle) {
    app.state::<AppState>().hardware.lock().unwrap().release();
}

#[tauri::command]
pub async fn system_stats(app: AppHandle) -> Result<HardwareInfo, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppState>().hardware.lock().unwrap().snapshot())
        .await
        .map_err(|e| e.to_string())
}

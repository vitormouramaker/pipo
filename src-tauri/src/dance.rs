//! Fase 8.6: o Pipo dança com a música. Enquanto alguma mídia estiver tocando no Windows,
//! o Pipo escuta o som do PC (loopback, só a energia do som, nada é gravado nem enviado),
//! acha as batidas pelos picos de volume e manda "pipo-beat" para a janela dele balançar.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tauri::{AppHandle, Emitter, Manager};

use crate::{windows, AppState};

static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Detector de batidas: compara a energia de cada trecho (~23 ms) com a média do último segundo.
pub struct Beats {
    history: Vec<f32>,
    last: Option<Instant>,
}

impl Beats {
    pub fn new() -> Self {
        Self { history: Vec::with_capacity(48), last: None }
    }

    /// Devolve a força da batida (0 a 1) quando o trecho é uma batida.
    pub fn feed(&mut self, energy: f32, now: Instant) -> Option<f32> {
        let average = if self.history.is_empty() { energy } else { self.history.iter().sum::<f32>() / self.history.len() as f32 };
        self.history.push(energy);
        if self.history.len() > 43 {
            self.history.remove(0);
        }
        let spaced = self.last.is_none_or(|t| now.duration_since(t) >= Duration::from_millis(280));
        if self.history.len() >= 20 && energy > 0.015 && energy > average * 1.45 && spaced {
            self.last = Some(now);
            return Some(((energy / average.max(1e-4) - 1.45) / 1.5).clamp(0.15, 1.0));
        }
        None
    }
}

impl Default for Beats {
    fn default() -> Self {
        Self::new()
    }
}

fn open_stream(app: &AppHandle) -> Option<cpal::Stream> {
    let device = cpal::default_host().default_output_device()?;
    let supported = device.default_output_config().ok()?;
    if supported.sample_format() != cpal::SampleFormat::F32 {
        return None;
    }
    let channels = supported.channels().max(1) as usize;
    let hop = (supported.sample_rate() as usize / 43).max(64) * channels;
    let app = app.clone();
    let mut beats = Beats::new();
    let mut acc = 0.0f32;
    let mut count = 0usize;
    let stream = device
        .build_input_stream::<f32, _, _>(
            supported.config(),
            move |data: &[f32], _| {
                for s in data {
                    acc += s * s;
                    count += 1;
                    if count >= hop {
                        let energy = (acc / count as f32).sqrt();
                        acc = 0.0;
                        count = 0;
                        if let Some(strength) = beats.feed(energy, Instant::now()) {
                            let _ = app.emit_to(windows::MAIN, "pipo-beat", strength);
                        }
                    }
                }
            },
            |e| eprintln!("dança: {e}"),
            None,
        )
        .ok()?;
    stream.play().ok()?;
    Some(stream)
}

/// Liga e desliga a escuta conforme a música toca (verifica a cada 3 s).
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let mut stream: Option<cpal::Stream> = None;
        loop {
            std::thread::sleep(Duration::from_secs(3));
            let state = app.state::<AppState>();
            let enabled = state.settings.lock().unwrap().dance;
            let gaming = state.sense.lock().unwrap().gaming;
            let visible = app.get_webview_window(windows::MAIN).is_some_and(|w| w.is_visible().unwrap_or(false));
            let should = enabled && !gaming && visible && crate::world::media::is_playing();
            if should && stream.is_none() {
                stream = open_stream(&app);
                ACTIVE.store(stream.is_some(), Ordering::SeqCst);
            } else if !should && stream.is_some() {
                stream = None;
                ACTIVE.store(false, Ordering::SeqCst);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acha_picos_e_respeita_o_intervalo() {
        let mut b = Beats::new();
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        // Um segundo de som constante: nenhuma batida.
        for i in 0..30 {
            assert!(b.feed(0.05, ms(i * 23)).is_none());
        }
        // Pico forte: batida.
        assert!(b.feed(0.2, ms(700)).is_some());
        // Outro pico logo em seguida (100 ms): ignorado.
        assert!(b.feed(0.2, ms(800)).is_none());
        // Depois do intervalo mínimo: batida de novo.
        for i in 0..5 {
            b.feed(0.05, ms(820 + i * 23));
        }
        assert!(b.feed(0.2, ms(1100)).is_some());
        // Silêncio não vira batida.
        let mut q = Beats::new();
        for i in 0..30 {
            q.feed(0.001, ms(i * 23));
        }
        assert!(q.feed(0.005, ms(800)).is_none());
    }
}

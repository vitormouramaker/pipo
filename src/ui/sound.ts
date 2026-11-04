import { listen } from "@tauri-apps/api/event";
import { api, type Settings } from "../api";

// Efeitos sonoros do Pipo, sintetizados na hora com Web Audio: sons curtos, macios e
// arredondados (senoides e triângulos com ataque rápido e queda suave). Sem arquivos.

let ctx: AudioContext | null = null;
let enabled = true;
let volume = 0.6;

function audio(): AudioContext | null {
  if (!enabled || volume <= 0) return null;
  ctx ??= new AudioContext();
  if (ctx.state === "suspended") void ctx.resume();
  return ctx;
}

type Note = { from: number; to?: number; at?: number; dur: number; type?: OscillatorType; gain?: number };

function play(notes: Note[]) {
  const ac = audio();
  if (!ac) return;
  const master = ac.createGain();
  master.gain.value = volume * 0.5;
  master.connect(ac.destination);
  const now = ac.currentTime + 0.01;
  for (const n of notes) {
    const start = now + (n.at ?? 0);
    const osc = ac.createOscillator();
    const env = ac.createGain();
    osc.type = n.type ?? "sine";
    osc.frequency.setValueAtTime(n.from, start);
    if (n.to) osc.frequency.exponentialRampToValueAtTime(n.to, start + n.dur);
    const peak = n.gain ?? 0.6;
    env.gain.setValueAtTime(0.0001, start);
    env.gain.exponentialRampToValueAtTime(peak, start + 0.008);
    env.gain.exponentialRampToValueAtTime(0.0001, start + n.dur);
    osc.connect(env).connect(master);
    osc.start(start);
    osc.stop(start + n.dur + 0.02);
  }
}

/** Sopro curto (ruído filtrado subindo), para "mensagem enviada". */
function whoosh() {
  const ac = audio();
  if (!ac) return;
  const length = Math.floor(ac.sampleRate * 0.18);
  const buffer = ac.createBuffer(1, length, ac.sampleRate);
  const data = buffer.getChannelData(0);
  for (let i = 0; i < length; i++) data[i] = (Math.random() * 2 - 1) * (1 - i / length);
  const src = ac.createBufferSource();
  src.buffer = buffer;
  const filter = ac.createBiquadFilter();
  filter.type = "bandpass";
  filter.Q.value = 1.2;
  const t = ac.currentTime;
  filter.frequency.setValueAtTime(600, t);
  filter.frequency.exponentialRampToValueAtTime(2600, t + 0.16);
  const gain = ac.createGain();
  gain.gain.value = volume * 0.18;
  src.connect(filter).connect(gain).connect(ac.destination);
  src.start();
}

export const sfx = {
  /** Menu radial abrindo: bolha subindo. */
  open: () => play([{ from: 520, to: 880, dur: 0.09 }, { from: 880, to: 1180, at: 0.05, dur: 0.07, gain: 0.3 }]),
  /** Menu fechando: bolha descendo. */
  close: () => play([{ from: 760, to: 420, dur: 0.08, gain: 0.45 }]),
  /** Pegou o Pipo. */
  pick: () => play([{ from: 300, to: 470, dur: 0.07, type: "triangle", gain: 0.5 }]),
  /** Soltou o Pipo: "bloop" com um balanço. */
  drop: () =>
    play([
      { from: 460, to: 230, dur: 0.12, type: "triangle", gain: 0.55 },
      { from: 300, to: 360, at: 0.1, dur: 0.08, gain: 0.25 },
    ]),
  /** Toque leve de interface (abas, opções). */
  tick: () => play([{ from: 1250, to: 1150, dur: 0.03, gain: 0.22 }]),
  send: whoosh,
  /** Resposta pronta. */
  reply: () => play([{ from: 660, dur: 0.12, gain: 0.4 }, { from: 990, at: 0.08, dur: 0.18, gain: 0.35 }]),
  /** Aviso chegando. */
  notify: () =>
    play([
      { from: 1047, dur: 0.12, gain: 0.35 },
      { from: 1319, at: 0.07, dur: 0.12, gain: 0.32 },
      { from: 1568, at: 0.14, dur: 0.2, gain: 0.3 },
    ]),
  /** Subiu de nível: arpejo com brilho. */
  level: () =>
    play([
      { from: 523, dur: 0.12, type: "triangle" },
      { from: 659, at: 0.08, dur: 0.12, type: "triangle" },
      { from: 784, at: 0.16, dur: 0.12, type: "triangle" },
      { from: 1047, at: 0.24, dur: 0.3, type: "triangle" },
      { from: 2093, at: 0.3, dur: 0.25, gain: 0.15 },
    ]),
  achievement: () =>
    play([
      { from: 784, dur: 0.1 },
      { from: 988, at: 0.07, dur: 0.1 },
      { from: 1175, at: 0.14, dur: 0.25, gain: 0.45 },
    ]),
  /** Algo deu errado: dois tons descendo, sem susto. */
  error: () => play([{ from: 392, dur: 0.12, type: "triangle", gain: 0.45 }, { from: 294, at: 0.1, dur: 0.18, type: "triangle", gain: 0.4 }]),
  /** Microfone abriu / fechou. */
  listen: () => play([{ from: 600, dur: 0.07, gain: 0.4 }, { from: 900, at: 0.07, dur: 0.1, gain: 0.4 }]),
  stopListen: () => play([{ from: 900, dur: 0.07, gain: 0.35 }, { from: 600, at: 0.07, dur: 0.1, gain: 0.35 }]),
  /** Ação concluída (lembrete criado, canal seguido…). */
  done: () => play([{ from: 880, to: 1320, dur: 0.1, gain: 0.35 }]),
};

/** Lê as preferências de som e acompanha as mudanças (uma vez por janela). */
export function setupSound() {
  const apply = (s: Settings) => {
    enabled = s.sounds;
    volume = Math.max(0, Math.min(1, s.soundVolume / 100));
  };
  void api.getSettings().then(apply);
  void listen<Settings>("settings-changed", (e) => apply(e.payload));
}

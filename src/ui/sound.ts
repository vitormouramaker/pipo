import { listen } from "@tauri-apps/api/event";
import { api, type Settings } from "../api";

// Efeitos sonoros do Pipo, sintetizados com Web Audio, na mesma receita do sintetizador
// do mmorpg-fantasia: camadas de tons e sinos FM com ataque suave, filtro passa-baixa
// (nada estridente acima de ~3 kHz), um reverb curto que "arredonda" o som e
// compressor + limitador no fim para nunca estourar no ouvido. Sem arquivos de áudio.

const SILENT = 0.0001;
const mf = (m: number) => 440 * Math.pow(2, (m - 69) / 12);

type Graph = { ctx: AudioContext; dry: GainNode; wet: GainNode; master: GainNode; noise: AudioBuffer };
let graph: Graph | null = null;
let enabled = true;
let volume = 0.6;

/** Reverb gerado: ruído decaindo e cada vez mais abafado (cauda escura, sem chiado). */
function impulse(ctx: AudioContext, sec: number, decay: number, damp: number) {
  const sr = ctx.sampleRate;
  const len = Math.floor(sr * sec);
  const buf = ctx.createBuffer(2, len, sr);
  for (let c = 0; c < 2; c++) {
    const d = buf.getChannelData(c);
    let lp = 0;
    for (let i = 0; i < len; i++) {
      const x = i / len;
      const a = Math.min(1, i / (sr * 0.012)) * Math.pow(1 - x, decay);
      lp += (Math.random() * 2 - 1 - lp) * (0.15 + (1 - damp) * 0.85 * (1 - x));
      d[i] = lp * a;
    }
  }
  return buf;
}

function setup(): Graph | null {
  if (!enabled || volume <= 0) return null;
  if (!graph) {
    const ctx = new AudioContext();
    const master = ctx.createGain();
    // Tira o brilho agressivo de tudo que passa.
    const tone = ctx.createBiquadFilter();
    tone.type = "lowpass";
    tone.frequency.value = 3200;
    tone.Q.value = 0.5;
    const comp = ctx.createDynamicsCompressor();
    comp.threshold.value = -20;
    comp.knee.value = 12;
    comp.ratio.value = 3;
    comp.attack.value = 0.01;
    comp.release.value = 0.25;
    const limiter = ctx.createDynamicsCompressor();
    limiter.threshold.value = -6;
    limiter.knee.value = 0;
    limiter.ratio.value = 20;
    limiter.attack.value = 0.002;
    limiter.release.value = 0.1;
    const dry = ctx.createGain();
    const wet = ctx.createGain();
    const reverb = ctx.createConvolver();
    reverb.buffer = impulse(ctx, 1.1, 4, 0.65);
    wet.gain.value = 0.35;
    dry.connect(tone);
    wet.connect(reverb).connect(tone);
    tone.connect(comp).connect(limiter).connect(master).connect(ctx.destination);
    const noise = ctx.createBuffer(1, ctx.sampleRate, ctx.sampleRate);
    const nd = noise.getChannelData(0);
    for (let i = 0; i < nd.length; i++) nd[i] = Math.random() * 2 - 1;
    graph = { ctx, dry, wet, master, noise };
  }
  // Compressor do Chrome soma ganho automático: o volume final fica bem baixo.
  graph.master.gain.value = volume * 0.35;
  if (graph.ctx.state === "suspended") void graph.ctx.resume();
  return graph;
}

/** Envelope suave: sobe em `a`, cai exponencialmente em `d`. Devolve o fim. */
function env(p: AudioParam, t: number, a: number, d: number, v: number) {
  p.setValueAtTime(SILENT, t);
  p.exponentialRampToValueAtTime(v, t + a);
  p.setTargetAtTime(SILENT, t + a, d / 4);
  return t + a + d + 0.05;
}

function out(g: Graph, node: AudioNode, reverb: number) {
  node.connect(g.dry);
  if (reverb > 0) {
    const send = g.ctx.createGain();
    send.gain.value = reverb;
    node.connect(send).connect(g.wet);
  }
}

type Tone = { f: number; f2?: number; w?: OscillatorType; a?: number; d: number; v: number; lp?: number; rev?: number };

function tone(g: Graph, t: number, o: Tone) {
  const osc = g.ctx.createOscillator();
  osc.type = o.w ?? "sine";
  osc.frequency.setValueAtTime(o.f, t);
  if (o.f2) osc.frequency.exponentialRampToValueAtTime(o.f2, t + o.d * 0.8);
  const filter = g.ctx.createBiquadFilter();
  filter.type = "lowpass";
  filter.frequency.value = o.lp ?? 2400;
  const gain = g.ctx.createGain();
  const end = env(gain.gain, t, o.a ?? 0.012, o.d, o.v);
  osc.connect(filter).connect(gain);
  out(g, gain, o.rev ?? 0.25);
  osc.start(t);
  osc.stop(end);
}

/** Sino FM macio (índice baixo = som redondo, sem metal). */
function bell(g: Graph, t: number, midi: number, d: number, v: number, ratio = 2, index = 0.8) {
  const f = mf(midi);
  const car = g.ctx.createOscillator();
  const mod = g.ctx.createOscillator();
  const modGain = g.ctx.createGain();
  const gain = g.ctx.createGain();
  car.frequency.value = f;
  mod.frequency.value = f * ratio;
  modGain.gain.setValueAtTime(f * index, t);
  modGain.gain.exponentialRampToValueAtTime(Math.max(1, f * 0.02), t + d * 0.6);
  const end = env(gain.gain, t, 0.006, d, v);
  mod.connect(modGain).connect(car.frequency);
  car.connect(gain);
  out(g, gain, 0.45);
  car.start(t);
  mod.start(t);
  car.stop(end);
  mod.stop(end);
}

/** Sopro: ruído filtrado que passa de grave para médio (nunca agudo). */
function breath(g: Graph, t: number, from: number, to: number, d: number, v: number) {
  const src = g.ctx.createBufferSource();
  src.buffer = g.noise;
  const filter = g.ctx.createBiquadFilter();
  filter.type = "bandpass";
  filter.Q.value = 0.9;
  filter.frequency.setValueAtTime(from, t);
  filter.frequency.exponentialRampToValueAtTime(to, t + d);
  const gain = g.ctx.createGain();
  const end = env(gain.gain, t, d * 0.4, d, v);
  src.connect(filter).connect(gain);
  out(g, gain, 0.2);
  src.start(t, Math.random() * 0.5);
  src.stop(end);
}

function play(fn: (g: Graph, t: number) => void) {
  const g = setup();
  if (g) fn(g, g.ctx.currentTime + 0.01);
}

export const sfx = {
  /** Menu abrindo: bolha macia subindo. */
  open: () =>
    play((g, t) => {
      tone(g, t, { f: 420, f2: 640, w: "triangle", d: 0.12, v: 0.26, lp: 1800 });
      breath(g, t, 500, 1400, 0.08, 0.05);
    }),
  /** Menu fechando: a mesma bolha descendo. */
  close: () =>
    play((g, t) => {
      tone(g, t, { f: 620, f2: 400, w: "triangle", d: 0.12, v: 0.22, lp: 1600 });
      breath(g, t, 1300, 500, 0.08, 0.04);
    }),
  /** Pegou o Pipo: "bup" grave. */
  pick: () => play((g, t) => tone(g, t, { f: 220, f2: 330, d: 0.1, v: 0.3, lp: 1200, rev: 0.15 })),
  /** Soltou o Pipo: "blup" com uma batidinha macia. */
  drop: () =>
    play((g, t) => {
      tone(g, t, { f: 330, f2: 165, d: 0.16, v: 0.3, lp: 1100, rev: 0.2 });
      tone(g, t + 0.09, { f: 247, f2: 262, d: 0.1, v: 0.12, lp: 1100 });
    }),
  /** Toque de interface (abas, opções): quase um sussurro. */
  tick: () => play((g, t) => tone(g, t, { f: 880, f2: 760, d: 0.05, v: 0.1, lp: 1500, rev: 0.1 })),
  /** Mensagem enviada: sopro curto. */
  send: () => play((g, t) => breath(g, t, 400, 1600, 0.16, 0.14)),
  /** Resposta pronta: dois sinos. */
  reply: () =>
    play((g, t) => {
      bell(g, t, 76, 0.6, 0.06);
      bell(g, t + 0.09, 83, 0.8, 0.05);
    }),
  /** Aviso chegando. */
  notify: () =>
    play((g, t) => {
      bell(g, t, 72, 0.7, 0.06);
      bell(g, t + 0.08, 76, 0.7, 0.055);
      bell(g, t + 0.16, 79, 1.0, 0.05);
    }),
  /** Subiu de nível: arpejo de sinos com brilho no fim. */
  level: () =>
    play((g, t) => {
      [72, 76, 79, 84].forEach((m, i) => bell(g, t + i * 0.09, m, 1.0, 0.06));
      bell(g, t + 0.38, 88, 1.4, 0.035, 3, 0.5);
      tone(g, t, { f: 131, d: 0.9, v: 0.05, lp: 600, a: 0.05, rev: 0.4 });
    }),
  achievement: () =>
    play((g, t) => {
      [79, 83, 86].forEach((m, i) => bell(g, t + i * 0.08, m, 1.0, 0.055));
    }),
  /** Algo deu errado: dois tons graves e redondos, sem susto. */
  error: () =>
    play((g, t) => {
      tone(g, t, { f: 294, d: 0.18, v: 0.08, lp: 900 });
      tone(g, t + 0.12, { f: 220, d: 0.26, v: 0.08, lp: 900 });
    }),
  /** Microfone abriu / fechou. */
  listen: () =>
    play((g, t) => {
      bell(g, t, 67, 0.4, 0.08);
      bell(g, t + 0.08, 74, 0.5, 0.08);
    }),
  stopListen: () =>
    play((g, t) => {
      bell(g, t, 74, 0.4, 0.07);
      bell(g, t + 0.08, 67, 0.5, 0.07);
    }),
  /** Ação concluída (lembrete criado, canal seguido…). */
  done: () => play((g, t) => bell(g, t, 79, 0.6, 0.1)),
  /** Pipo encolhendo para a ilha / voltando dela. */
  shrink: () => play((g, t) => tone(g, t, { f: 520, f2: 260, w: "triangle", d: 0.22, v: 0.22, lp: 1400 })),
  grow: () => play((g, t) => tone(g, t, { f: 260, f2: 520, w: "triangle", d: 0.22, v: 0.22, lp: 1400 })),
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

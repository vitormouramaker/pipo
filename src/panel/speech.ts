import { api, type Settings } from "../api";

// Fala do Pipo. Duas vozes:
// - "natural": Piper, voz neural que roda no próprio PC (baixada uma vez). Bem menos robótica.
// - "windows": as vozes do Windows (ex.: Microsoft Maria), a alternativa sem download.
// Tudo pode ser interrompido a qualquer momento (botão, Esc, clique no Pipo, atalho de voz).

export function portugueseVoices(): SpeechSynthesisVoice[] {
  return window.speechSynthesis.getVoices().filter((v) => v.lang.toLowerCase().startsWith("pt"));
}

/** As vozes do Windows carregam de forma assíncrona na primeira chamada. */
export function loadVoices(): Promise<SpeechSynthesisVoice[]> {
  const now = portugueseVoices();
  if (now.length) return Promise.resolve(now);
  return new Promise((resolve) => {
    const done = () => resolve(portugueseVoices());
    window.speechSynthesis.addEventListener("voiceschanged", done, { once: true });
    window.setTimeout(done, 1500);
  });
}

/** Tira o que não faz sentido ler: blocos de código, links e marcações. */
function forSpeech(text: string): string {
  return text
    .replace(/```[\s\S]*?```/g, " (trecho de código) ")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/https?:\/\/\S+/g, "")
    .replace(/[*_#>|]+/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 2000);
}

/** Frases curtas: a primeira começa a tocar enquanto as próximas são geradas. */
function sentences(text: string): string[] {
  const parts = text.match(/[^.!?…]+[.!?…]+["')\]]*|[^.!?…]+$/g) ?? [text];
  const out: string[] = [];
  for (const p of parts.map((s) => s.trim()).filter(Boolean)) {
    // Junta pedaços muito curtos para a voz não ficar picada.
    if (out.length && out[out.length - 1].length < 40) out[out.length - 1] += ` ${p}`;
    else out.push(p);
  }
  return out;
}

// ---------- Estado da fala (para o botão "Parar de falar") ----------
let speaking = false;
let session = 0;
let audio: HTMLAudioElement | null = null;
const listeners = new Set<(speaking: boolean) => void>();

function setSpeaking(value: boolean) {
  if (speaking === value) return;
  speaking = value;
  void api.petExpress(value ? "speaking" : "clear");
  listeners.forEach((fn) => fn(value));
}

export function onSpeakingChange(fn: (speaking: boolean) => void): () => void {
  listeners.add(fn);
  fn(speaking);
  return () => listeners.delete(fn);
}

export function isSpeaking() {
  return speaking;
}

export function stopSpeaking() {
  session++;
  if (audio) {
    audio.pause();
    audio.src = "";
    audio = null;
  }
  window.speechSynthesis.cancel();
  setSpeaking(false);
}

function playWav(base64: string, id: number): Promise<void> {
  return new Promise((resolve) => {
    if (id !== session) return resolve();
    const a = new Audio(`data:audio/wav;base64,${base64}`);
    audio = a;
    a.onended = () => resolve();
    a.onerror = () => resolve();
    void a.play().catch(() => resolve());
  });
}

/** Devolve false se a voz natural não funcionou (aí usa a do Windows). */
async function speakNatural(text: string, id: number): Promise<boolean> {
  const parts = sentences(text);
  // Gera a próxima frase enquanto a atual toca.
  let next: Promise<string | null> = api.ttsSynth(parts[0]).catch(() => null);
  for (let i = 0; i < parts.length; i++) {
    const wav = await next;
    if (id !== session) return true;
    if (!wav && i === 0) return false;
    next = i + 1 < parts.length ? api.ttsSynth(parts[i + 1]).catch(() => null) : Promise.resolve(null);
    if (wav) await playWav(wav, id);
    if (id !== session) return true;
  }
  return true;
}

async function speakWindows(text: string, voiceName: string, id: number) {
  const voices = await loadVoices();
  if (id !== session) return;
  await new Promise<void>((resolve) => {
    const utterance = new SpeechSynthesisUtterance(text);
    utterance.lang = "pt-BR";
    utterance.voice = voices.find((v) => v.name === voiceName) ?? voices.find((v) => v.lang === "pt-BR") ?? voices[0] ?? null;
    utterance.rate = 1.05;
    utterance.onend = () => resolve();
    utterance.onerror = () => resolve();
    window.speechSynthesis.cancel();
    window.speechSynthesis.speak(utterance);
  });
}

/** Lê o texto com a voz escolhida nas configurações. */
export async function speak(text: string, settings?: Pick<Settings, "voiceEngine" | "voiceName">) {
  const clean = forSpeech(text);
  if (!clean) return;
  stopSpeaking();
  const id = ++session;
  const s = settings ?? (await api.getSettings());
  setSpeaking(true);
  try {
    const natural = s.voiceEngine === "natural" && (await api.ttsStatus()).ready;
    const ok = natural && (await speakNatural(clean, id));
    if (!ok && id === session) await speakWindows(clean, s.voiceName, id);
  } finally {
    if (id === session) setSpeaking(false);
  }
}

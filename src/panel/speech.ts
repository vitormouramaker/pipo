import { api } from "../api";

// Fala usa as vozes do próprio Windows (ex.: Microsoft Maria, pt-BR): offline e sem custo.

export function portugueseVoices(): SpeechSynthesisVoice[] {
  return window.speechSynthesis.getVoices().filter((v) => v.lang.toLowerCase().startsWith("pt"));
}

/** As vozes carregam de forma assíncrona na primeira chamada. */
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
    .replace(/[*_#>]+/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 1200);
}

export async function speak(text: string, voiceName: string) {
  const clean = forSpeech(text);
  if (!clean) return;
  const voices = await loadVoices();
  const utterance = new SpeechSynthesisUtterance(clean);
  utterance.lang = "pt-BR";
  utterance.voice = voices.find((v) => v.name === voiceName) ?? voices.find((v) => v.lang === "pt-BR") ?? voices[0] ?? null;
  utterance.rate = 1.05;
  utterance.onstart = () => void api.petExpress("speaking");
  utterance.onend = () => void api.petExpress("clear");
  utterance.onerror = () => void api.petExpress("clear");
  window.speechSynthesis.cancel();
  window.speechSynthesis.speak(utterance);
}

export function stopSpeaking() {
  if (window.speechSynthesis.speaking) {
    window.speechSynthesis.cancel();
    void api.petExpress("clear");
  }
}

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type MiniSource } from "../api";
import { ICONS } from "../ui/icons";
import "./mini.css";

const PLATFORM: Record<string, string> = { twitch: "Twitch", youtube: "YouTube", kick: "Kick" };

/** Link do player oficial de cada plataforma, feito para ser embutido. */
function playerUrl(src: MiniSource): string | null {
  if (src.platform === "twitch") {
    // A Twitch exige o domínio da página que embute o player (aqui, tauri.localhost).
    return `https://player.twitch.tv/?channel=${encodeURIComponent(src.handle)}&parent=${location.hostname}&autoplay=true&muted=false`;
  }
  if (src.platform === "kick") return `https://player.kick.com/${encodeURIComponent(src.handle)}?autoplay=true`;
  if (src.video) return `https://www.youtube.com/embed/${src.video}?autoplay=1&playsinline=1`;
  return null;
}

// Mini-player de lives (Fase 7.3): barra para arrastar e o player embaixo.
export default function MiniApp() {
  const [src, setSrc] = useState<MiniSource | null>(null);

  useEffect(() => {
    document.documentElement.style.background = "#0f1114";
    document.body.style.background = "#0f1114";
    void api.miniSource().then(setSrc);
    const unlisten = listen<MiniSource>("mini-load", (e) => setSrc(e.payload));
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && void api.miniClose();
    window.addEventListener("keydown", onKey);
    return () => {
      void unlisten.then((fn) => fn());
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  if (!src) return <div className="mini" />;
  const url = playerUrl(src);
  const name = src.handle.replace(/^@/, "");
  return (
    <div className="mini">
      <header className="mini-bar" data-tauri-drag-region>
        <span className="mini-dot" data-tauri-drag-region />
        <span className="mini-title" data-tauri-drag-region title={src.title ?? undefined}>
          <strong data-tauri-drag-region>{name}</strong>
          <span data-tauri-drag-region> · {PLATFORM[src.platform] ?? src.platform}</span>
          {src.title && <span className="mini-sub" data-tauri-drag-region> — {src.title}</span>}
        </span>
        <button title="Legenda ao vivo (traduzida)" aria-label="Legenda ao vivo" onClick={() => void api.captionsToggle()}>
          CC
        </button>
        <button title="Abrir no navegador" aria-label="Abrir no navegador" onClick={() => void api.openUrl(src.url)}>
          {ICONS.external}
        </button>
        <button title="Fechar (Esc)" aria-label="Fechar" onClick={() => void api.miniClose()}>
          {ICONS.close}
        </button>
      </header>
      {url ? (
        <iframe
          key={url}
          className="mini-player"
          src={url}
          title={`Live de ${name}`}
          allow="autoplay; fullscreen; encrypted-media; picture-in-picture"
          allowFullScreen
        />
      ) : (
        <p className="mini-empty">Não achei a live agora.</p>
      )}
    </div>
  );
}

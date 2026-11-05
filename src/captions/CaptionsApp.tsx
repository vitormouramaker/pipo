import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type Settings } from "../api";
import { ICONS } from "../ui/icons";
import "./captions.css";

type Caption = { id: number; original: string; translated: string | null; lang: string };

const LANG: Record<string, string> = { en: "inglês", es: "espanhol", fr: "francês", de: "alemão", ja: "japonês", ko: "coreano", it: "italiano", ru: "russo", zh: "chinês", pt: "português" };

// Fase 8.5: faixa de legenda ao vivo (o som do PC, transcrito no próprio computador).
export default function CaptionsApp() {
  const [lines, setLines] = useState<Caption[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void api.getSettings().then(setSettings);
    const unlisten = [
      listen<Caption>("caption", (e) => {
        const c = e.payload;
        setError(null);
        setLines((all) => {
          const i = all.findIndex((x) => x.id === c.id);
          if (i >= 0) return all.map((x) => (x.id === c.id ? { ...x, ...c } : x));
          return [...all, c].slice(-2);
        });
      }),
      listen<string>("captions-error", (e) => setError(e.payload)),
      listen<Settings>("settings-changed", (e) => setSettings(e.payload)),
    ];
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && void api.captionsToggle();
    window.addEventListener("keydown", onKey);
    return () => {
      unlisten.forEach((u) => void u.then((fn) => fn()));
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  const size = settings?.captionsSize ?? 22;
  const showOriginal = settings?.captionsOriginal ?? true;
  const set = (patch: Partial<Settings>) => void api.settingsUpdate(patch).then(setSettings);
  const last = lines[lines.length - 1];

  return (
    <div className="captions" style={{ fontSize: size }}>
      <div className="captions-bar" data-tauri-drag-region>
        <span className="captions-live" data-tauri-drag-region>
          <i /> {last && last.lang !== "pt" && LANG[last.lang] ? `ouvindo ${LANG[last.lang]}` : "legenda ao vivo"}
        </span>
        <button title="Letra menor" onClick={() => set({ captionsSize: Math.max(14, size - 2) })}>
          A−
        </button>
        <button title="Letra maior" onClick={() => set({ captionsSize: Math.min(40, size + 2) })}>
          A+
        </button>
        <button title={showOriginal ? "Esconder o texto original" : "Mostrar o texto original"} className={showOriginal ? "on" : undefined} onClick={() => set({ captionsOriginal: !showOriginal })}>
          Original
        </button>
        <button title="Fechar (Esc)" aria-label="Fechar" onClick={() => void api.captionsToggle()}>
          {ICONS.close}
        </button>
      </div>
      <div className="captions-text" data-tauri-drag-region>
        {error && <p className="captions-error">{error}</p>}
        {!error && lines.length === 0 && <p className="captions-wait">Ouvindo o som do PC… (o que tocar aparece aqui)</p>}
        {lines.map((c, i) => (
          <div key={c.id} className={`captions-line${i === lines.length - 1 ? " now" : ""}`}>
            <p className="captions-main">{c.lang === "pt" || !settings?.captionsTranslate ? c.original : c.translated ?? <span className="captions-pending">{c.original}</span>}</p>
            {showOriginal && c.lang !== "pt" && settings?.captionsTranslate && c.translated && <p className="captions-original">{c.original}</p>}
          </div>
        ))}
      </div>
    </div>
  );
}

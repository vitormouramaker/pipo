import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type ClipItem } from "../api";
import { ICONS } from "../ui/icons";
import { sfx } from "../ui/sound";

type Props = {
  enabled: boolean;
  onAsk: (text: string) => void;
  onRun: (prompt: string) => void;
  onOpenSettings: () => void;
};

// Ações rápidas com o texto copiado: vão direto para o chat.
const QUICK: { label: string; prompt: (t: string) => string }[] = [
  { label: "Resumir", prompt: (t) => `Resuma em tópicos curtos:\n\n${t}` },
  { label: "Traduzir", prompt: (t) => `Traduza para o português do Brasil (ou para o inglês, se já estiver em português):\n\n${t}` },
  { label: "Revisar", prompt: (t) => `Revise a ortografia, a gramática e a clareza, e mostre a versão corrigida:\n\n${t}` },
];

type Filter = "all" | "pinned" | "images";
const fold = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

export default function Clipboard({ enabled, onAsk, onRun, onOpenSettings }: Props) {
  const [items, setItems] = useState<ClipItem[]>([]);
  const [copied, setCopied] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [zoom, setZoom] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState("Ctrl+Alt+Shift+T");

  const refresh = () => void api.clipboardList().then(setItems);

  useEffect(() => {
    refresh();
    void api.getSettings().then((s) => setShortcut(s.plainPasteShortcut));
    const unlisten = listen("clipboard-changed", refresh);
    return () => void unlisten.then((fn) => fn());
  }, []);

  if (!enabled) {
    return (
      <section className="clipboard">
        <p className="empty">
          O histórico da área de transferência está desligado.{" "}
          <button className="link" onClick={onOpenSettings}>
            Ligar nas configurações
          </button>
        </p>
      </section>
    );
  }

  const q = fold(query.trim());
  const shown = items.filter(
    (i) => (filter === "all" || (filter === "pinned" ? i.pinned : i.kind === "image")) && (!q || fold(i.text).includes(q)),
  );
  const copy = (item: ClipItem) =>
    void api.clipboardCopy(item.id).then(() => {
      sfx.tick();
      setCopied(item.id);
      window.setTimeout(() => setCopied(null), 1500);
    });

  return (
    <section className="clipboard">
      <div className="clip-top">
        <div className="settings-search clip-search">
          {ICONS.search}
          <input type="search" placeholder="Procurar no que você copiou" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Procurar" />
        </div>
        <div className="segmented" aria-label="Filtro">
          {(
            [
              ["all", "Tudo"],
              ["pinned", "Fixados"],
              ["images", "Imagens"],
            ] as const
          ).map(([id, label]) => (
            <button key={id} aria-pressed={filter === id} onClick={() => setFilter(id)}>
              {label}
            </button>
          ))}
        </div>
      </div>
      <div className="chat-toolbar">
        <span className="hint">
          Fica só neste PC e some ao fechar o Pipo (menos os fixados); senhas são ignoradas. <kbd>{shortcut}</kbd> cola como texto puro.
        </span>
        {items.some((i) => !i.pinned) && (
          <button className="link" onClick={() => void api.clipboardClear().then(refresh)}>
            Limpar
          </button>
        )}
      </div>
      <div className="clip-list">
        {shown.length === 0 && (
          <p className="empty">{items.length === 0 ? "Copie um texto ou uma imagem e ele aparece aqui." : "Nada encontrado."}</p>
        )}
        {shown.map((item) => (
          <div key={item.id} className={`clip-item${item.pinned ? " pinned" : ""}`}>
            {item.kind === "image" && item.thumb ? (
              <button className="thumb clip-thumb" title="Ver maior" onClick={() => setZoom(item.thumb!)}>
                <img src={item.thumb} alt={item.text} />
              </button>
            ) : (
              <pre className="clip-text">{item.text}</pre>
            )}
            <div className="clip-actions">
              <span className="clip-time">{item.kind === "image" ? `${item.text} · ${item.at}` : item.at}</span>
              <button className="clip-copy" title={item.kind === "text" ? "Copia como texto puro, sem formatação" : "Copiar a imagem"} onClick={() => copy(item)}>
                {copied === item.id ? "Copiado ✓" : "Copiar"}
              </button>
              {item.kind === "text" ? (
                <>
                  {QUICK.map((qk) => (
                    <button key={qk.label} className="link" onClick={() => onRun(qk.prompt(item.text))}>
                      {qk.label}
                    </button>
                  ))}
                  <button className="link" onClick={() => onAsk(item.text)}>
                    Perguntar
                  </button>
                </>
              ) : (
                <button className="link" onClick={() => void api.clipboardAskImage(item.id).then(() => api.panelOpen("chat"))}>
                  Perguntar ao Pipo
                </button>
              )}
              <span className="clip-icons">
                <button
                  className={`icon-btn clip-pin${item.pinned ? " on" : ""}`}
                  title={item.pinned ? "Desafixar" : item.kind === "text" ? "Fixar (fica guardado mesmo depois de fechar o Pipo)" : "Fixar nesta sessão"}
                  aria-label={item.pinned ? "Desafixar" : "Fixar"}
                  aria-pressed={item.pinned}
                  onClick={() => void api.clipboardPin(item.id, !item.pinned).then(refresh)}
                >
                  📌
                </button>
                <button className="icon-btn" title="Excluir" aria-label="Excluir" onClick={() => void api.clipboardRemove(item.id).then(refresh)}>
                  {ICONS.close}
                </button>
              </span>
            </div>
          </div>
        ))}
      </div>
      {zoom && (
        <div className="lightbox" role="dialog" aria-label="Imagem copiada" onClick={() => setZoom(null)}>
          <img src={zoom} alt="Imagem copiada" />
        </div>
      )}
    </section>
  );
}

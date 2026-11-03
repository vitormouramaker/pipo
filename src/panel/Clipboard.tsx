import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type ClipItem } from "../api";

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

export default function Clipboard({ enabled, onAsk, onRun, onOpenSettings }: Props) {
  const [items, setItems] = useState<ClipItem[]>([]);
  const [copied, setCopied] = useState<number | null>(null);

  const refresh = () => void api.clipboardList().then(setItems);

  useEffect(() => {
    refresh();
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

  return (
    <section className="clipboard">
      <div className="chat-toolbar">
        <span className="hint">Só texto, só neste computador; some ao fechar o Pipo. Senhas e tokens são ignorados.</span>
        {items.length > 0 && (
          <button className="link" onClick={() => void api.clipboardClear().then(refresh)}>
            Limpar
          </button>
        )}
      </div>
      <div className="clip-list">
        {items.length === 0 && <p className="empty">Copie algum texto e ele aparece aqui.</p>}
        {items.map((item) => (
          <div key={item.id} className="clip-item">
            <pre className="clip-text">{item.text}</pre>
            <div className="clip-actions">
              <span className="clip-time">{item.at}</span>
              <button
                className="link"
                onClick={() =>
                  void api.clipboardCopy(item.id).then(() => {
                    setCopied(item.id);
                    window.setTimeout(() => setCopied(null), 1500);
                  })
                }
              >
                {copied === item.id ? "Copiado ✓" : "Copiar"}
              </button>
              {QUICK.map((q) => (
                <button key={q.label} className="link" onClick={() => onRun(q.prompt(item.text))}>
                  {q.label}
                </button>
              ))}
              <button className="link" onClick={() => onAsk(item.text)}>
                Perguntar
              </button>
              <button className="link muted" onClick={() => void api.clipboardRemove(item.id).then(refresh)}>
                Excluir
              </button>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

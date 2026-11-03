import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type ChatMessage } from "../api";

type Props = { hasKey: boolean | null; onOpenSettings: () => void };

// Formatação mínima: blocos ```código```, `código` em linha e **negrito**.
function renderInline(text: string): ReactNode[] {
  return text.split(/(`[^`\n]+`|\*\*[^*\n]+\*\*)/g).map((part, i) => {
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2) return <code key={i}>{part.slice(1, -1)}</code>;
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4) return <strong key={i}>{part.slice(2, -2)}</strong>;
    return <Fragment key={i}>{part}</Fragment>;
  });
}

function renderText(text: string): ReactNode[] {
  return text.split(/```/).map((chunk, i) => {
    if (i % 2 === 1) {
      const code = chunk.replace(/^[\w+-]*\n/, "");
      return <pre key={i}>{code}</pre>;
    }
    return <span key={i}>{renderInline(chunk)}</span>;
  });
}

export default function Chat({ hasKey, onOpenSettings }: Props) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    void api.chatHistory().then(setMessages);
    inputRef.current?.focus();
    const unlisten = listen("panel-view", () => inputRef.current?.focus());
    return () => void unlisten.then((fn) => fn());
  }, []);

  useEffect(() => {
    listRef.current?.scrollTo({ top: listRef.current.scrollHeight });
  }, [messages]);

  const send = async () => {
    const text = input.trim();
    if (!text || busy) return;
    setInput("");
    setError(null);
    setBusy(true);
    setMessages((m) => [...m, { role: "user", text }, { role: "assistant", text: "" }]);

    const appendToLast = (piece: string) =>
      setMessages((m) => {
        const copy = m.slice();
        const last = copy[copy.length - 1];
        copy[copy.length - 1] = { ...last, text: last.text + piece };
        return copy;
      });

    await api.chatSend(text, (event) => {
      if (event.type === "delta") appendToLast(event.text);
      else if (event.type === "error") setError(event.message);
      else if (event.stopReason === "refusal") setError("O Pipo não pode responder a esse pedido.");
      else if (event.stopReason === "max_tokens") setError("A resposta foi cortada por ser longa demais.");
    });

    setMessages((m) => (m[m.length - 1]?.text ? m : m.slice(0, -1)));
    setBusy(false);
    inputRef.current?.focus();
  };

  const newConversation = async () => {
    await api.chatNew();
    setMessages([]);
    setError(null);
    inputRef.current?.focus();
  };

  return (
    <section className="chat">
      {hasKey === false && (
        <div className="banner">
          Configure a chave da Claude API para conversar.
          <button className="link" onClick={onOpenSettings}>
            Abrir configurações
          </button>
        </div>
      )}

      <div className="messages" ref={listRef}>
        {messages.length === 0 && (
          <div className="empty">
            <p className="empty-title">Olá. Sou o Pipo.</p>
            <p>Pergunte o que quiser: código, documentos, jogos ou o que estiver acontecendo no mundo.</p>
          </div>
        )}
        {messages.map((m, i) => (
          <div key={i} className={`msg ${m.role}`}>
            {m.text ? renderText(m.text) : <span className="typing">pensando…</span>}
          </div>
        ))}
        {error && <div className="msg error">{error}</div>}
      </div>

      <footer className="composer">
        <textarea
          ref={inputRef}
          rows={2}
          value={input}
          placeholder="Mensagem para o Pipo  (Enter envia, Shift+Enter quebra linha)"
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void send();
            }
          }}
        />
        <div className="composer-row">
          <button className="link" onClick={() => void newConversation()} disabled={busy}>
            Nova conversa
          </button>
          <button className="primary" onClick={() => void send()} disabled={busy || !input.trim()}>
            {busy ? "Respondendo…" : "Enviar"}
          </button>
        </div>
      </footer>
    </section>
  );
}

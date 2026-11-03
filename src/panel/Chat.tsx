import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type ChatMessage, type Conversation } from "../api";
import { sqliteDate } from "../format";

type Props = {
  hasKey: boolean | null;
  draft: string | null;
  onDraftUsed: () => void;
  onOpenSettings: () => void;
};

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

export default function Chat({ hasKey, draft, onDraftUsed, onOpenSettings }: Props) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<Conversation[] | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<number | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const busyRef = useRef(false);

  useEffect(() => {
    void api.chatHistory().then(setMessages);
    inputRef.current?.focus();
    // Texto pronto vindo do menu do Pipo: pode ter chegado antes deste painel existir.
    const takePrefill = () =>
      void api.chatTakePrefill().then((text) => {
        if (text) void sendText(text);
      });
    takePrefill();
    const unlistenView = listen("panel-view", () => inputRef.current?.focus());
    const unlistenPrefill = listen("chat-prefill", takePrefill);
    return () => {
      void unlistenView.then((fn) => fn());
      void unlistenPrefill.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (!draft) return;
    setInput(draft);
    onDraftUsed();
    inputRef.current?.focus();
  }, [draft]);

  useEffect(() => {
    listRef.current?.scrollTo({ top: listRef.current.scrollHeight });
  }, [messages, history]);

  const send = () => {
    const text = input.trim();
    if (!text || busy) return;
    setInput("");
    void sendText(text);
  };

  const sendText = async (text: string) => {
    if (busyRef.current) return;
    busyRef.current = true;
    setHistory(null);
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
    busyRef.current = false;
    setBusy(false);
    inputRef.current?.focus();
  };

  const newConversation = async () => {
    await api.chatNew();
    setMessages([]);
    setError(null);
    setHistory(null);
    inputRef.current?.focus();
  };

  const toggleHistory = async () => {
    setConfirmDelete(null);
    setHistory(history ? null : await api.chatConversations());
  };

  const openConversation = async (id: number) => {
    setMessages(await api.chatOpen(id));
    setError(null);
    setHistory(null);
    inputRef.current?.focus();
  };

  const deleteConversation = async (id: number) => {
    await api.chatDelete(id);
    setConfirmDelete(null);
    setHistory(await api.chatConversations());
    if (history?.find((c) => c.id === id)?.current) setMessages(await api.chatHistory());
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

      <div className="chat-toolbar">
        <button className="link" aria-pressed={history !== null} onClick={() => void toggleHistory()} disabled={busy}>
          {history ? "← Voltar à conversa" : "Histórico"}
        </button>
        <button className="link" onClick={() => void newConversation()} disabled={busy}>
          + Nova conversa
        </button>
      </div>

      {history ? (
        <div className="history">
          {history.length === 0 && <p className="empty">Nenhuma conversa ainda.</p>}
          {history.map((c) => (
            <div key={c.id} className={`history-item${c.current ? " current" : ""}`}>
              <button className="history-open" onClick={() => void openConversation(c.id)}>
                <span className="history-title">{c.title}</span>
                <span className="history-date">{sqliteDate(c.updatedAt)}</span>
              </button>
              {confirmDelete === c.id ? (
                <button className="history-delete confirm" onClick={() => void deleteConversation(c.id)}>
                  Confirmar
                </button>
              ) : (
                <button className="history-delete" title="Excluir conversa" onClick={() => setConfirmDelete(c.id)}>
                  Excluir
                </button>
              )}
            </div>
          ))}
        </div>
      ) : (
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
      )}

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
          <span />
          <button className="primary" onClick={() => void send()} disabled={busy || !input.trim()}>
            {busy ? "Respondendo…" : "Enviar"}
          </button>
        </div>
      </footer>
    </section>
  );
}

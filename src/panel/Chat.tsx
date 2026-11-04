import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type Attachment, type ChatMessage, type Conversation, type VoiceState } from "../api";
import { onSpeakingChange, speak, stopSpeaking } from "./speech";
import { ICONS } from "../ui/icons";
import { sfx } from "../ui/sound";
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
  const [voice, setVoice] = useState<VoiceState>({ phase: "idle", message: null });
  const [talking, setTalking] = useState(false);
  useEffect(() => onSpeakingChange(setTalking), []);
  // Arquivos anexados (soltos no Pipo ou no painel) que vão com a próxima mensagem.
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  // Miniaturas das imagens anexadas (caminho -> data URL) e a imagem ampliada agora.
  const [previews, setPreviews] = useState<Record<string, string>>({});
  const [zoom, setZoom] = useState<string | null>(null);
  const [download, setDownload] = useState<{ percent: number; error: string | null } | null>(null);

  // Baixa o modelo de voz sem sair do chat.
  const downloadModel = () => {
    setDownload({ percent: 0, error: null });
    const timer = window.setInterval(() => {
      void api.voiceStatus().then((s) => {
        if (s.total > 0) setDownload((d) => d && { ...d, percent: Math.floor((100 * s.downloaded) / s.total) });
      });
    }, 500);
    void api
      .voiceDownload()
      .then(() => {
        setDownload(null);
        setVoice({ phase: "idle", message: "Modelo pronto! Clique no microfone e fale." });
      })
      .catch((e) => setDownload({ percent: 0, error: String(e) }))
      .finally(() => window.clearInterval(timer));
  };
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const busyRef = useRef(false);

  useEffect(() => {
    void api.chatHistory().then(setMessages);
    inputRef.current?.focus();
    // Texto pronto vindo do menu do Pipo: pode ter chegado antes deste painel existir.
    const takePrefill = () =>
      void api.chatTakePrefill().then((prefill) => {
        if (!prefill) return;
        if (prefill.fresh) setMessages([]);
        void sendText(prefill.text, prefill.voice);
      });
    takePrefill();
    const unlistenView = listen("panel-view", () => inputRef.current?.focus());
    const unlistenPrefill = listen("chat-prefill", takePrefill);
    const unlistenVoice = listen<VoiceState>("voice-state", (e) => setVoice(e.payload));
    const takeAttachments = () =>
      void api.chatTakeAttachments().then((fresh) => {
        if (!fresh.length) return;
        sfx.done();
        setAttachments((a) => [...a, ...fresh.filter((f) => !a.some((x) => x.path === f.path))].slice(0, 5));
        inputRef.current?.focus();
      });
    takeAttachments();
    const unlistenAttach = listen("chat-attach", takeAttachments);
    return () => {
      void unlistenView.then((fn) => fn());
      void unlistenPrefill.then((fn) => fn());
      void unlistenVoice.then((fn) => fn());
      void unlistenAttach.then((fn) => fn());
      stopSpeaking();
    };
  }, []);

  useEffect(() => {
    for (const a of attachments) {
      if (a.kind !== "image" || previews[a.path]) continue;
      void api
        .attachmentPreview(a.path)
        .then((url) => setPreviews((p) => ({ ...p, [a.path]: url })))
        .catch(() => {});
    }
  }, [attachments]);

  useEffect(() => {
    if (!zoom) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopImmediatePropagation();
        setZoom(null);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [zoom]);

  useEffect(() => {
    if (!draft) return;
    setInput(draft);
    onDraftUsed();
    inputRef.current?.focus();
  }, [draft]);

  useEffect(() => {
    listRef.current?.scrollTo({ top: listRef.current.scrollHeight });
  }, [messages, history]);

  const send = (preset?: string) => {
    const text = (preset ?? input).trim();
    if ((!text && attachments.length === 0) || busy) return;
    setInput("");
    void sendText(text);
  };

  const sendText = async (text: string, spoken = false) => {
    if (busyRef.current) return;
    const files = attachments;
    setAttachments([]);
    const isThumb = (f: Attachment) => f.kind === "image" && Boolean(previews[f.path]);
    const images = files.filter(isThumb).map((f) => previews[f.path]);
    const shown = [...files.filter((f) => !isThumb(f)).map((f) => `📎 ${f.name}`), text].filter(Boolean).join("\n");
    stopSpeaking();
    sfx.send();
    let reply = "";
    let failed = false;
    busyRef.current = true;
    setHistory(null);
    setError(null);
    setBusy(true);
    setMessages((m) => [...m, { role: "user", text: shown, images }, { role: "assistant", text: "" }]);

    // A resposta pode alternar texto e uso de ferramentas: cada trecho de texto vira um balão.
    const appendText = (piece: string) =>
      setMessages((m) => {
        const last = m[m.length - 1];
        if (last?.role !== "assistant") return [...m, { role: "assistant", text: piece }];
        return [...m.slice(0, -1), { ...last, text: last.text + piece }];
      });
    // Fontes citadas ficam no último balão de texto da resposta.
    const addSource = (source: { title: string; url: string }) =>
      setMessages((m) => {
        const i = m.map((msg) => msg.role).lastIndexOf("assistant");
        if (i < 0 || m[i].sources?.some((s) => s.url === source.url)) return m;
        const copy = m.slice();
        copy[i] = { ...m[i], sources: [...(m[i].sources ?? []), source] };
        return copy;
      });
    const pushStep = (step: ChatMessage) =>
      setMessages((m) => {
        const last = m[m.length - 1];
        const base = last?.role === "assistant" && !last.text ? m.slice(0, -1) : m;
        return [...base, step, { role: "assistant", text: "" }];
      });

    await api.chatSend(text, (event) => {
      if (event.type === "delta") {
        reply += event.text;
        appendText(event.text);
      }
      else if (event.type === "tool") pushStep({ role: "tool", text: event.label });
      else if (event.type === "confirm") pushStep({ role: "confirm", text: event.question, confirmId: event.id });
      else if (event.type === "source") addSource({ title: event.title, url: event.url });
      else if (event.type === "error") {
        failed = true;
        setError(event.message);
      }
      else if (event.stopReason === "refusal") setError("O Pipo não pode responder a esse pedido.");
      else if (event.stopReason === "max_tokens") setError("A resposta foi cortada por ser longa demais.");
    }, files.map((f) => f.path));

    setMessages((m) => m.filter((msg, i) => msg.role !== "assistant" || msg.text || i < m.length - 1));
    busyRef.current = false;
    setBusy(false);
    inputRef.current?.focus();
    if (failed) sfx.error();
    else sfx.reply();
    // Pergunta feita por voz: a resposta também pode ser falada.
    if (spoken && reply.trim()) {
      const settings = await api.getSettings();
      if (settings.speakReplies) void speak(reply, settings);
    }
  };

  const decide = (id: string, approved: boolean) => {
    void api.chatConfirm(id, approved);
    setMessages((m) => m.map((msg) => (msg.confirmId === id ? { ...msg, decision: approved ? "approved" : "denied" } : msg)));
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
              <p>
                Pergunte o que quiser ou peça uma ação: “roda os testes do projeto”, “o que está usando a porta 3000?”,
                “me lembra em 20 minutos de alongar”.
              </p>
            </div>
          )}
          {messages.map((m, i) => {
            if (m.role === "tool") {
              return (
                <div key={i} className="msg tool">
                  ⚙ {m.text}
                </div>
              );
            }
            if (m.role === "confirm") {
              return (
                <div key={i} className="msg confirm">
                  <span className="confirm-title">O Pipo quer fazer uma ação</span>
                  <p>{m.text}</p>
                  {m.decision ? (
                    <span className={`confirm-result ${m.decision}`}>
                      {m.decision === "approved" ? "Permitido" : "Negado"}
                    </span>
                  ) : (
                    <div className="confirm-actions">
                      <button className="primary" onClick={() => decide(m.confirmId!, true)}>
                        Permitir
                      </button>
                      <button onClick={() => decide(m.confirmId!, false)}>Negar</button>
                    </div>
                  )}
                </div>
              );
            }
            return (
              <div key={i} className={`msg ${m.role}`}>
                {m.images && m.images.length > 0 && (
                  <div className="msg-images">
                    {m.images.map((src, n) => (
                      <button key={n} className="thumb" title="Ver imagem inteira" onClick={() => setZoom(src)}>
                        <img src={src} alt="Imagem anexada" />
                      </button>
                    ))}
                  </div>
                )}
                {m.text ? renderText(m.text) : m.images?.length ? null : <span className="typing">pensando…</span>}
                {m.sources && m.sources.length > 0 && (
                  <div className="sources">
                    <span>Fontes:</span>
                    {m.sources.slice(0, 8).map((s, n) => (
                      <button key={s.url} className="link source" title={s.url} onClick={() => void api.openUrl(s.url)}>
                        [{n + 1}] {s.title}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
          {error && <div className="msg error">{error}</div>}
        </div>
      )}

      {zoom && (
        <div className="lightbox" role="dialog" aria-label="Imagem anexada" onClick={() => setZoom(null)}>
          <img src={zoom} alt="Imagem anexada" />
          <button className="lightbox-close" aria-label="Fechar" onClick={() => setZoom(null)}>
            {ICONS.close}
          </button>
        </div>
      )}

      <footer className="composer">
        {talking && (
          <button className="stop-speaking" onClick={stopSpeaking} title="Também para com Esc ou clicando no Pipo">
            ■ Parar de falar
          </button>
        )}
        {attachments.length > 0 && (
          <div className="attachments">
            {attachments.map((a) => (
              <span key={a.path} className={`attachment-chip${previews[a.path] ? " has-thumb" : ""}`} title={a.path}>
                {previews[a.path] ? (
                  <button className="thumb small" title="Ver imagem inteira" onClick={() => setZoom(previews[a.path])}>
                    <img src={previews[a.path]} alt={a.name} />
                  </button>
                ) : (
                  <>📎 {a.name}</>
                )}
                <button aria-label={`Tirar ${a.name}`} onClick={() => setAttachments((all) => all.filter((x) => x.path !== a.path))}>
                  {ICONS.close}
                </button>
              </span>
            ))}
            <div className="attachment-actions">
              {[
                ["Resumir", "Resuma em tópicos curtos."],
                ["Pontos principais", "Quais são os pontos principais e o que eu preciso fazer?"],
                ["Traduzir", "Traduza para o português do Brasil (se já estiver em português, para o inglês)."],
                ["Explicar", "Explique de forma simples, como se eu nunca tivesse visto isso."],
              ].map(([label, prompt]) => (
                <button key={label} className="link" disabled={busy} onClick={() => send(prompt)}>
                  {label}
                </button>
              ))}
            </div>
          </div>
        )}
        <textarea
          ref={inputRef}
          rows={2}
          value={input}
          placeholder={attachments.length ? "Pergunte sobre o anexo ou use um atalho acima" : "Mensagem para o Pipo  (Enter envia, Shift+Enter quebra linha)"}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void send();
            }
          }}
        />
        <div className="composer-row">
          <span className={voice.phase === "error" ? "voice-status error" : "voice-status"}>
            {voice.phase === "listening" && "Ouvindo… fale e faça uma pausa para enviar"}
            {voice.phase === "transcribing" && "Transcrevendo…"}
            {(voice.phase === "error" || voice.phase === "idle") && voice.message}
            {voice.phase === "nomodel" &&
              (download ? (
                download.error ?? `Baixando o modelo de voz… ${download.percent}%`
              ) : (
                <>
                  Para falar, baixe o modelo de voz (142 MB, uma vez só).{" "}
                  <button className="link" onClick={downloadModel}>
                    Baixar agora
                  </button>
                </>
              ))}
          </span>
          <button
            className={voice.phase === "listening" ? "mic on" : "mic"}
            title={voice.phase === "listening" ? "Parar e enviar" : "Falar com o Pipo (o áudio é transcrito no computador)"}
            aria-label="Falar com o Pipo"
            disabled={busy || voice.phase === "transcribing" || download !== null}
            onClick={() => {
              stopSpeaking();
              void api.voiceToggle();
            }}
          >
            {voice.phase === "listening" ? ICONS.stop : ICONS.mic}
          </button>
          <button className="primary" onClick={() => void send()} disabled={busy || (!input.trim() && attachments.length === 0)}>
            {busy ? "Respondendo…" : "Enviar"}
          </button>
        </div>
      </footer>
    </section>
  );
}

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { api, type WriteSession } from "../api";
import { Pipo } from "../pipo/Pipo";
import { sfx } from "../ui/sound";
import "./write.css";

// Ações com o texto selecionado (o número é o atalho de teclado).
const ACTIONS = [
  { id: "fix", label: "Corrigir", hint: "ortografia e gramática" },
  { id: "translate", label: "Traduzir", hint: "português ↔ inglês" },
  { id: "summary", label: "Resumir", hint: "em poucas linhas" },
  { id: "formal", label: "Mais formal", hint: "tom profissional" },
  { id: "short", label: "Mais curto", hint: "direto ao ponto" },
  { id: "friendly", label: "Simpático", hint: "tom leve" },
];
// Estas não substituem a seleção por padrão (Enter copia).
const COPY_FIRST = new Set(["summary"]);

type Phase = { kind: "pick" } | { kind: "loading"; action: string } | { kind: "done"; action: string; text: string } | { kind: "error"; message: string };

/** Fase 9.6: janelinha do atalho "texto selecionado" (corrigir, traduzir, resumir…). */
export default function WriteApp() {
  const [session, setSession] = useState<WriteSession | null>(null);
  const [phase, setPhase] = useState<Phase>({ kind: "pick" });
  const [custom, setCustom] = useState("");
  const [last, setLast] = useState<{ action: string; custom?: string } | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const load = () =>
      void api.writePending().then((s) => {
        if (!s) return;
        setSession(s);
        setPhase({ kind: "pick" });
        setCustom("");
      });
    load();
    const unlisten = listen("write-show", load);
    // Clicou fora: fecha (a seleção no app continua como estava).
    const unfocus = getCurrentWindow().onFocusChanged(({ payload }) => {
      if (!payload) void api.writeClose();
    });
    return () => {
      void unlisten.then((fn) => fn());
      void unfocus.then((fn) => fn());
    };
  }, []);

  // A janela acompanha o tamanho do conteúdo.
  useLayoutEffect(() => {
    if (!session || !box.current) return;
    const send = () => {
      const r = box.current!.getBoundingClientRect();
      void api.writeLayout(Math.ceil(r.width), Math.ceil(r.height));
    };
    send();
    const observer = new ResizeObserver(send);
    observer.observe(box.current);
    return () => observer.disconnect();
  }, [session]);

  const run = (action: string, text?: string) => {
    setLast({ action, custom: text });
    setPhase({ kind: "loading", action });
    void api
      .writeRun(action, text)
      .then((out) => {
        sfx.done();
        setPhase({ kind: "done", action, text: out });
      })
      .catch((e) => setPhase({ kind: "error", message: String(e) }));
  };
  const apply = (mode: "replace" | "copy") => {
    if (phase.kind !== "done") return;
    void api.writeApply(phase.text, mode);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (phase.kind === "done" || phase.kind === "error") setPhase({ kind: "pick" });
        else void api.writeClose();
        return;
      }
      if (document.activeElement === input.current) return;
      if (phase.kind === "pick" && /^[1-6]$/.test(e.key)) {
        e.preventDefault();
        run(ACTIONS[Number(e.key) - 1].id);
      }
      if (phase.kind === "done" && e.key === "Enter") {
        e.preventDefault();
        apply(COPY_FIRST.has(phase.action) ? "copy" : "replace");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!session) return null;
  const label = (id: string) => ACTIONS.find((a) => a.id === id)?.label ?? "Pedido";
  const words = session.text.trim().split(/\s+/).length;

  return (
    <div className="write" ref={box}>
      <header className="write-head" data-tauri-drag-region>
        <span className="write-face">
          <Pipo state={phase.kind === "loading" ? "thinking" : phase.kind === "done" ? "done" : "attentive"} />
        </span>
        <span className="write-title" data-tauri-drag-region>
          {phase.kind === "pick" && <>Texto selecionado{session.appName ? ` no ${session.appName}` : ""}</>}
          {phase.kind === "loading" && <>{label(phase.action)}…</>}
          {phase.kind === "done" && <>{label(phase.action)}</>}
          {phase.kind === "error" && <>Não deu certo</>}
          <small>{words} {words === 1 ? "palavra" : "palavras"}</small>
        </span>
        <button className="write-close" aria-label="Fechar" title="Fechar (Esc)" onClick={() => void api.writeClose()}>
          ✕
        </button>
      </header>

      {phase.kind === "pick" && (
        <>
          <p className="write-preview">{session.text}</p>
          {!session.hasKey && <p className="write-error">Configure a chave da Claude API no painel do Pipo para usar isto.</p>}
          <div className="write-actions">
            {ACTIONS.map((a, i) => (
              <button key={a.id} disabled={!session.hasKey} onClick={() => run(a.id)} title={a.hint}>
                <kbd>{i + 1}</kbd>
                {a.label}
              </button>
            ))}
          </div>
          <form
            className="write-custom"
            onSubmit={(e) => {
              e.preventDefault();
              if (custom.trim()) run("custom", custom);
            }}
          >
            <input
              ref={input}
              placeholder='Ou peça: "em espanhol", "vira um e-mail", "em tópicos"…'
              value={custom}
              disabled={!session.hasKey}
              onChange={(e) => setCustom(e.target.value)}
            />
          </form>
        </>
      )}

      {phase.kind === "loading" && <p className="write-wait">O Pipo está escrevendo…</p>}

      {phase.kind === "done" && (
        <>
          <div className="write-result" tabIndex={0}>
            {phase.text}
          </div>
          <div className="write-buttons">
            {COPY_FIRST.has(phase.action) ? (
              <>
                <button className="primary" onClick={() => apply("copy")}>
                  Copiar <kbd>Enter</kbd>
                </button>
                <button onClick={() => apply("replace")}>Substituir</button>
              </>
            ) : (
              <>
                <button className="primary" onClick={() => apply("replace")}>
                  Substituir <kbd>Enter</kbd>
                </button>
                <button onClick={() => apply("copy")}>Copiar</button>
              </>
            )}
            {last && (
              <button className="link" onClick={() => run(last.action, last.custom)}>
                De novo
              </button>
            )}
            <button className="link" onClick={() => setPhase({ kind: "pick" })}>
              Voltar
            </button>
          </div>
        </>
      )}

      {phase.kind === "error" && (
        <>
          <p className="write-error">{phase.message}</p>
          <div className="write-buttons">
            <button onClick={() => setPhase({ kind: "pick" })}>Voltar</button>
          </div>
        </>
      )}
    </div>
  );
}

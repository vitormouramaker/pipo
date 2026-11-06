import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type Toast } from "../api";
import { Pipo, type PipoState } from "../pipo/Pipo";
import { ICONS } from "../ui/icons";
import { sfx } from "../ui/sound";
import "./toast.css";

// Quanto tempo cada balão fica na tela (pausa com o mouse em cima). O tempo só começa a
// contar quando o balão aparece: com mais de MAX avisos, os outros esperam na fila.
const DURATION = 7000;
const MAX = 3;

const FACE: Record<string, PipoState> = {
  live: "attentive",
  reminder: "attentive",
  pomodoro: "done",
  break: "idle",
  alert: "alert",
  error: "alert",
  level: "levelup",
  achievement: "done",
  done: "done",
  info: "idle",
  lol: "attentive",
};
const TONE: Record<string, string> = { alert: "warn", error: "warn", level: "gold", achievement: "gold", live: "live" };

function Bubble({ toast, onClose }: { toast: Toast; onClose: () => void }) {
  const [paused, setPaused] = useState(false);
  // Dicas do LoL são mais longas: ficam mais tempo.
  const duration = toast.kind === "lol" ? 16000 : DURATION;
  const left = useRef(duration);
  const started = useRef(Date.now());

  useEffect(() => {
    if (paused) return;
    started.current = Date.now();
    const timer = window.setTimeout(onClose, left.current);
    return () => {
      window.clearTimeout(timer);
      left.current -= Date.now() - started.current;
    };
  }, [paused]);

  const clickable = Boolean(toast.url || toast.view || toast.prompt || toast.mini);
  // No máximo duas linhas: título e texto em uma linha cada (ou o título em duas, sem texto).
  return (
    <div
      className={`bubble ${TONE[toast.kind] ?? ""}${clickable ? " clickable" : ""}${toast.body ? " has-body" : ""}`}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onClick={() => {
        if (!clickable) return;
        void api.toastOpen(toast.url, toast.view, toast.prompt, toast.mini);
        onClose();
      }}
    >
      <div className="face">
        <Pipo state={FACE[toast.kind] ?? "idle"} />
      </div>
      <div className="text">
        <strong>{toast.title}</strong>
        {toast.body && <span>{toast.body}</span>}
        {toast.habit != null && (
          <button
            className="bubble-action"
            onClick={(e) => {
              e.stopPropagation();
              sfx.done();
              void api.habitLog(toast.habit!, 1);
              onClose();
            }}
          >
            Feito ✓
          </button>
        )}
      </div>
      <button
        className="close"
        aria-label="Fechar aviso"
        onClick={(e) => {
          e.stopPropagation();
          onClose();
        }}
      >
        {ICONS.close}
      </button>
      <div className="timer" style={{ animationDuration: `${duration}ms`, animationPlayState: paused ? "paused" : "running" }} />
    </div>
  );
}

export default function ToastApp() {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [ready, setReady] = useState(false);
  const stack = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const take = () =>
      void api.toastTake().then((fresh) => {
        if (fresh.length) {
          const kinds = fresh.map((t) => t.kind);
          if (kinds.includes("level")) sfx.level();
          else if (kinds.includes("achievement")) sfx.achievement();
          else if (kinds.includes("error")) sfx.error();
          else sfx.notify();
          setToasts((t) => [...t, ...fresh]);
        }
        setReady(true);
      });
    take();
    const unlisten = listen("toast-new", take);
    return () => void unlisten.then((fn) => fn());
  }, []);

  // A janela acompanha a altura dos balões; sem balões, ela some.
  useLayoutEffect(() => {
    if (!ready) return;
    if (toasts.length === 0) {
      void api.toastDone();
      return;
    }
    const height = stack.current?.getBoundingClientRect().height ?? 0;
    void api.toastLayout(Math.ceil(height));
  }, [toasts, ready]);

  const waiting = Math.max(0, toasts.length - MAX);
  return (
    <div className="toast-stack" ref={stack}>
      {toasts.slice(0, MAX).map((t) => (
        <Bubble key={t.id} toast={t} onClose={() => setToasts((all) => all.filter((x) => x.id !== t.id))} />
      ))}
      {waiting > 0 && (
        <div className="toast-more">
          +{waiting} {waiting === 1 ? "aviso" : "avisos"} na fila
        </div>
      )}
    </div>
  );
}

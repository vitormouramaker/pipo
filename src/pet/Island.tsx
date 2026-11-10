import { useEffect, useRef, useState, type CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type FocusStatus } from "../api";
import type { PipoState } from "../pipo/Pipo";
import { ICONS } from "../ui/icons";
import { skinColors } from "../pipo/Pipo";
import { useDragPhysics } from "./physics";
import { sfx } from "../ui/sound";

export type Edge = "top" | "bottom" | "left" | "right" | "top-left" | "top-right" | "bottom-left" | "bottom-right";

// Tamanho da janela da ilha (igual ao island.rs) e da pílula, normal e expandida.
const WINDOW = { h: [250, 52], v: [52, 210] } as const;
const PILL = { h: { small: [120, 28], big: [236, 46] }, v: { small: [28, 112], big: [46, 196] } } as const;
const times = ([w, h]: readonly [number, number], k: number) => [w * k, h * k] as const;
const CLICK_TOLERANCE = 4;

/** Retângulo [x, y, largura, altura] da pílula dentro da janela, grudada na borda ou no canto
 *  (o island.rs calcula o mesmo para saber quando o mouse está em cima). */
function rect(edge: Edge, [w, h]: readonly [number, number], k: number): [number, number, number, number] {
  const vertical = edge === "left" || edge === "right";
  // Tamanho real da janela (o Rust usa o mesmo para saber se o mouse está em cima).
  const [ww, wh] = window.innerWidth > 0 ? [window.innerWidth, window.innerHeight] : times(vertical ? WINDOW.v : WINDOW.h, k);
  const x = edge.endsWith("left") ? 0 : edge.endsWith("right") ? ww - w : (ww - w) / 2;
  const y = edge.startsWith("top") ? 0 : edge.startsWith("bottom") ? wh - h : (wh - h) / 2;
  return [x, y, w, h];
}

const clock = () => new Date().toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" });
const mmss = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

// O Pipo encolhido: uma pílula grafite com os olhos dele, grudada na borda da tela.
/** Cores da ilha: as do Pipo, grafite (padrão antigo) ou vidro (translúcida). */
function islandColors(skin?: string, style?: string): Record<string, string> {
  const c = skinColors(skin);
  if (style === "vidro") return { "--island-bg": "rgba(18, 20, 23, 0.55)", "--island-border": "rgba(255, 255, 255, 0.14)", "--island-eye": c.eye, "--island-blur": "blur(10px)" };
  if (style === "grafite") return {};
  return { "--island-bg": c.bottom, "--island-border": c.edge, "--island-eye": c.eye };
}

export default function Island({
  edge,
  state,
  scale = 1,
  skin,
  islandStyle,
  gaze = [0, 0],
  hearts = null,
}: {
  edge: Edge;
  state: PipoState;
  /** Carinho: chave dos corações (null = sem corações). */
  hearts?: number | null;
  scale?: number;
  skin?: string;
  islandStyle?: string;
  gaze?: [number, number];
}) {
  // Movimento "gostosinho" ao arrastar pela borda (mesma física do Pipo, mais de leve).
  const motionRef = useRef<HTMLDivElement>(null);
  const beatRef = useRef<HTMLDivElement>(null);
  useDragPhysics(motionRef, { strength: 0.45 });
  // Batida da música (e comemoração): a pílula pulsa.
  const pulse = (strength: number) => {
    const el = beatRef.current;
    if (!el) return;
    el.style.setProperty("--beat", String(strength));
    el.classList.remove("beat");
    void el.offsetWidth;
    el.classList.add("beat");
  };
  useEffect(() => {
    const unlisten = listen<number>("pipo-beat", (e) => pulse(e.payload));
    return () => void unlisten.then((fn) => fn());
  }, []);
  // Vitória (levelup): três pulinhos.
  useEffect(() => {
    if (state !== "levelup") return;
    const timers = [0, 260, 520].map((ms) => window.setTimeout(() => pulse(1), ms));
    return () => timers.forEach((t) => window.clearTimeout(t));
  }, [state]);
  const vertical = edge === "left" || edge === "right";
  const [hover, setHover] = useState(false);
  // Redesenha quando a janela muda de tamanho (troca de borda, escala).
  const [, setSize] = useState(0);
  useEffect(() => {
    const onResize = () => setSize((n) => n + 1);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  // Brilho rápido ao virar ilha, para ela não sumir em fundos escuros.
  const [glow, setGlow] = useState(true);
  const [info, setInfo] = useState(clock());
  const press = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    const unlisten = listen<boolean>("island-hover", (e) => setHover(e.payload));
    const unlistenGlow = listen("island-glow", () => {
      sfx.find();
      setGlow(false);
      requestAnimationFrame(() => setGlow(true));
    });
    return () => {
      void unlisten.then((fn) => fn());
      void unlistenGlow.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (!glow) return;
    const timer = window.setTimeout(() => setGlow(false), 2000);
    return () => window.clearTimeout(timer);
  }, [glow]);

  // Expandida: mostra o pomodoro (se houver) ou a hora, com os hábitos do dia (ex.: "14:30 · 2/4 ✓").
  const [habits, setHabits] = useState("");
  useEffect(() => {
    if (!hover) return;
    void api.habitsList().then((list) => {
      const today = list.filter((h) => h.scheduled);
      setHabits(today.length ? ` · ${today.filter((h) => h.done).length}/${today.length} ✓` : "");
    });
    const update = () =>
      void api.focusStatus().then((s: FocusStatus) => {
        const p = s.pomodoro;
        setInfo(p ? `${p.phase === "focus" ? "Foco" : "Pausa"} ${mmss(p.remainingSecs)}` : clock());
      });
    update();
    const timer = window.setInterval(update, 1000);
    return () => window.clearInterval(timer);
  }, [hover]);

  const dims = times((vertical ? PILL.v : PILL.h)[hover ? "big" : "small"], scale);
  const [x, y, w, h] = rect(edge, dims, scale);

  const [gx, gy] = state === "sleep" || state === "gaming" ? [0, 0] : gaze;
  return (
    <main className="island-stage">
      <div className="island-motion" ref={motionRef} style={{ transformOrigin: `${x + w / 2}px ${y + h / 2}px` }}>
      <div className="island-beat" ref={beatRef} style={{ transformOrigin: `${x + w / 2}px ${y + h / 2}px` }}>
      <div
        className={`island ${edge}${vertical ? " splat-v" : ""} s-${state}${hover ? " hover" : ""}${glow ? " glow" : ""}`}
        style={{ left: x, top: y, width: w, height: h, "--k": scale, ...islandColors(skin, islandStyle) } as CSSProperties}
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          press.current = { x: e.screenX, y: e.screenY };
          void api.petDragStart();
        }}
        onPointerUp={(e) => {
          const p = press.current;
          press.current = null;
          if (!p || Math.hypot(e.screenX - p.x, e.screenY - p.y) > CLICK_TOLERANCE) return;
          if (state === "speaking") {
            void api.speechStop();
          } else if ((e.target as Element).closest(".island-exit")) {
            sfx.grow();
            void api.islandToggle();
          } else {
            sfx.tick();
            void api.panelToggle();
          }
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          void api.petMenuOpen();
        }}
      >
        <span className="island-eyes" style={{ transform: `translate(${(gx * 0.7 * scale).toFixed(1)}px, ${(gy * 0.45 * scale).toFixed(1)}px)` }}>
          {state === "listening" ? (
            <>
              <i className="bar" />
              <i className="bar" />
              <i className="bar" />
            </>
          ) : (
            <>
              <i />
              <i />
            </>
          )}
        </span>
        {hover && !vertical && (
          <span className="island-info">
            {info}
            {habits && <span className="island-habits">{habits}</span>}
          </span>
        )}
        {hover && (
          <button className="island-exit" aria-label="Voltar ao Pipo">
            {ICONS.expand}
          </button>
        )}
      </div>
      </div>
      </div>
      {hearts && (
        <span className="hearts-fx island-hearts" key={hearts} aria-hidden="true" style={{ left: x, top: y, width: w, height: h }}>
          <span>♥</span>
          <span>♥</span>
          <span>♥</span>
        </span>
      )}
    </main>
  );
}

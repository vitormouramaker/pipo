import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type FocusStatus } from "../api";
import type { PipoState } from "../pipo/Pipo";
import { ICONS } from "../ui/icons";
import { sfx } from "../ui/sound";

export type Edge = "top" | "bottom" | "left" | "right" | "top-left" | "top-right" | "bottom-left" | "bottom-right";

// Tamanho da janela da ilha (igual ao island.rs) e da pílula, normal e expandida.
const WINDOW = { h: [250, 52], v: [52, 210] } as const;
const PILL = { h: { small: [120, 28], big: [236, 46] }, v: { small: [28, 112], big: [46, 196] } } as const;
const CLICK_TOLERANCE = 4;

/** Retângulo [x, y, largura, altura] da pílula dentro da janela, grudada na borda ou no canto
 *  (o island.rs calcula o mesmo para saber quando o mouse está em cima). */
function rect(edge: Edge, [w, h]: readonly [number, number]): [number, number, number, number] {
  const vertical = edge === "left" || edge === "right";
  const [ww, wh] = vertical ? WINDOW.v : WINDOW.h;
  const x = edge.endsWith("left") ? 0 : edge.endsWith("right") ? ww - w : (ww - w) / 2;
  const y = edge.startsWith("top") ? 0 : edge.startsWith("bottom") ? wh - h : (wh - h) / 2;
  return [x, y, w, h];
}

const clock = () => new Date().toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" });
const mmss = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

// O Pipo encolhido: uma pílula grafite com os olhos dele, grudada na borda da tela.
export default function Island({ edge, state }: { edge: Edge; state: PipoState }) {
  const vertical = edge === "left" || edge === "right";
  const [hover, setHover] = useState(false);
  const [info, setInfo] = useState(clock());
  const press = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    const unlisten = listen<boolean>("island-hover", (e) => setHover(e.payload));
    return () => void unlisten.then((fn) => fn());
  }, []);

  // Expandida: mostra o pomodoro (se houver) ou a hora.
  useEffect(() => {
    if (!hover) return;
    const update = () =>
      void api.focusStatus().then((s: FocusStatus) => {
        const p = s.pomodoro;
        setInfo(p ? `${p.phase === "focus" ? "Foco" : "Pausa"} ${mmss(p.remainingSecs)}` : clock());
      });
    update();
    const timer = window.setInterval(update, 1000);
    return () => window.clearInterval(timer);
  }, [hover]);

  const dims = (vertical ? PILL.v : PILL.h)[hover ? "big" : "small"];
  const [x, y, w, h] = rect(edge, dims);

  return (
    <main className="island-stage">
      <div
        className={`island ${edge} s-${state}${hover ? " hover" : ""}`}
        style={{ left: x, top: y, width: w, height: h }}
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          press.current = { x: e.screenX, y: e.screenY };
          void api.petDragStart();
        }}
        onPointerUp={(e) => {
          const p = press.current;
          press.current = null;
          if (!p || Math.hypot(e.screenX - p.x, e.screenY - p.y) > CLICK_TOLERANCE) return;
          if ((e.target as Element).closest(".island-exit")) {
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
        <span className="island-eyes">
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
        {hover && !vertical && <span className="island-info">{info}</span>}
        {hover && (
          <button className="island-exit" aria-label="Voltar ao Pipo">
            {ICONS.expand}
          </button>
        )}
      </div>
    </main>
  );
}

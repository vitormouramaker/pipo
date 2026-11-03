import { useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Pipo, PIPO_STATES } from "./pipo/Pipo";

const DRAG_THRESHOLD = 3;

export default function App() {
  const [index, setIndex] = useState(0);
  const press = useRef<{ x: number; y: number; dragged: boolean } | null>(null);

  return (
    <main
      className="stage"
      title="Arraste para mover · clique para trocar o estado · botão direito para fechar"
      onMouseDown={(e) => {
        if (e.button === 0) press.current = { x: e.clientX, y: e.clientY, dragged: false };
      }}
      onMouseMove={(e) => {
        const p = press.current;
        if (!p || p.dragged || !(e.buttons & 1)) return;
        if (Math.hypot(e.clientX - p.x, e.clientY - p.y) > DRAG_THRESHOLD) {
          p.dragged = true;
          getCurrentWindow().startDragging();
        }
      }}
      onMouseUp={() => {
        if (press.current && !press.current.dragged) setIndex((i) => (i + 1) % PIPO_STATES.length);
        press.current = null;
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        getCurrentWindow().close();
      }}
    >
      <Pipo state={PIPO_STATES[index]} />
    </main>
  );
}

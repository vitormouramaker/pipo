import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Menu, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "../api";
import { Pipo, PIPO_STATES, type PipoState } from "../pipo/Pipo";

const DRAG_THRESHOLD = 3;
// Estados passageiros voltam sozinhos ao ocioso.
const TRANSIENT: Partial<Record<PipoState, number>> = { done: 2500, alert: 5000 };

async function showContextMenu() {
  const menu = await Menu.new({
    items: [
      { id: "chat", text: "Abrir chat", action: () => void api.panelOpen("chat") },
      { id: "settings", text: "Configurações", action: () => void api.panelOpen("settings") },
      { id: "hide", text: "Ocultar Pipo", action: () => void api.petToggle() },
      await PredefinedMenuItem.new({ item: "Separator" }),
      { id: "quit", text: "Sair", action: () => void api.quit() },
    ],
  });
  await menu.popup();
}

export default function PetApp() {
  const [state, setState] = useState<PipoState>("idle");
  const press = useRef<{ x: number; y: number; dragged: boolean } | null>(null);

  useEffect(() => {
    const unlisten = listen<string>("pipo-state", (e) => {
      if ((PIPO_STATES as readonly string[]).includes(e.payload)) setState(e.payload as PipoState);
    });
    return () => void unlisten.then((fn) => fn());
  }, []);

  useEffect(() => {
    const ms = TRANSIENT[state];
    if (!ms) return;
    const timer = window.setTimeout(() => setState("idle"), ms);
    return () => window.clearTimeout(timer);
  }, [state]);

  return (
    <main
      className="stage"
      title="Clique para abrir o chat · arraste para mover · botão direito para o menu"
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
        if (press.current && !press.current.dragged) void api.panelToggle();
        press.current = null;
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        void showContextMenu();
      }}
    >
      <Pipo state={state} />
    </main>
  );
}

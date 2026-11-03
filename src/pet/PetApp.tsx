import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Menu, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { api } from "../api";
import { Pipo, PIPO_STATES, type PipoState } from "../pipo/Pipo";

// Até quantos pixels de movimento ainda conta como clique (e não arrasto).
const CLICK_TOLERANCE = 4;
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
  const press = useRef<{ x: number; y: number } | null>(null);

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
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        // Coordenadas de tela: a janela se move durante o arrasto.
        press.current = { x: e.screenX, y: e.screenY };
        void api.petDragStart();
      }}
      onPointerUp={(e) => {
        const p = press.current;
        press.current = null;
        if (p && Math.hypot(e.screenX - p.x, e.screenY - p.y) <= CLICK_TOLERANCE) void api.panelToggle();
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

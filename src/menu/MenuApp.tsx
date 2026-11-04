import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import { ICONS } from "../ui/icons";
import { sfx } from "../ui/sound";
import "./menu.css";

type Item = { id: string; label: string; icon: ReactNode; action: () => void; danger?: boolean };

// Menu do botão direito: janela própria, ao lado do Pipo, com a identidade dele.
export default function MenuApp() {
  // null = fechado; true/false = aberto com o Pipo em modo ilha ou não.
  const [island, setIsland] = useState<boolean | null>(null);
  const [round, setRound] = useState(0);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void api.menuPending().then((pending) => {
      if (pending !== null) {
        setIsland(pending);
        setRound((r) => r + 1);
      }
    });
    const unlisten = listen<boolean>("menu-show", (e) => {
      setIsland(e.payload);
      setRound((r) => r + 1);
    });
    const close = () => {
      setIsland(null);
      void api.menuHide();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("blur", close);
    window.addEventListener("keydown", onKey);
    return () => {
      void unlisten.then((fn) => fn());
      window.removeEventListener("blur", close);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  // Mede o menu (sem a animação) e pede para a janela aparecer ao lado do Pipo.
  useLayoutEffect(() => {
    const node = box.current;
    if (island === null || !node) return;
    sfx.open();
    void api.menuLayout(Math.ceil(node.offsetWidth), Math.ceil(node.offsetHeight));
  }, [round]);

  if (island === null) return null;

  const run = (action: () => void) => () => {
    sfx.tick();
    setIsland(null);
    void api.menuHide();
    action();
  };
  const items: Item[] = [
    { id: "chat", label: "Abrir chat", icon: ICONS.chat, action: () => void api.panelOpen("chat") },
    { id: "mic", label: "Falar com o Pipo", icon: ICONS.mic, action: () => void api.voiceToggle() },
    { id: "screen", label: "Perguntar sobre a tela", icon: ICONS.crop, action: () => void api.screenAsk() },
    island
      ? { id: "island", label: "Voltar ao Pipo", icon: ICONS.expand, action: () => void api.islandToggle() }
      : { id: "island", label: "Modo ilha", icon: ICONS.island, action: () => void api.islandToggle() },
    { id: "pipo", label: "Evolução do Pipo", icon: ICONS.star, action: () => void api.panelOpen("pipo") },
    { id: "settings", label: "Configurações", icon: ICONS.settings, action: () => void api.panelOpen("settings") },
    { id: "hide", label: "Ocultar Pipo", icon: ICONS.hide, action: () => void api.petToggle() },
    { id: "quit", label: "Sair", icon: ICONS.power, action: () => void api.quit(), danger: true },
  ];

  return (
    <div className="menu" ref={box} key={round} role="menu">
      {items.map((item) => (
        <button key={item.id} role="menuitem" className={item.danger ? "danger" : undefined} onClick={run(item.action)}>
          {item.icon}
          {item.label}
        </button>
      ))}
    </div>
  );
}

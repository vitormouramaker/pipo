import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type PanelView } from "../api";
import { Pipo } from "../pipo/Pipo";
import Chat from "./Chat";
import SettingsView from "./Settings";
import "./panel.css";

export default function PanelApp() {
  const [view, setView] = useState<PanelView>("chat");
  const [hasKey, setHasKey] = useState<boolean | null>(null);

  const refreshKey = () => void api.keyStatus().then(setHasKey);

  useEffect(() => {
    refreshKey();
    const unlisten = listen<PanelView>("panel-view", (e) => {
      setView(e.payload);
      refreshKey();
    });
    return () => void unlisten.then((fn) => fn());
  }, []);

  return (
    <div className="panel">
      <header className="panel-header" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <span className="brand-icon">
            <Pipo state="idle" />
          </span>
          Pipo
        </div>
        <nav className="tabs">
          <button aria-pressed={view === "chat"} onClick={() => setView("chat")}>
            Chat
          </button>
          <button aria-pressed={view === "settings"} onClick={() => setView("settings")}>
            Configurações
          </button>
        </nav>
        <button className="icon-btn" title="Fechar" onClick={() => void api.panelHide()}>
          ✕
        </button>
      </header>

      {view === "chat" ? (
        <Chat hasKey={hasKey} onOpenSettings={() => setView("settings")} />
      ) : (
        <SettingsView onKeyChange={refreshKey} />
      )}
    </div>
  );
}

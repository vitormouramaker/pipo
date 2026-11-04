import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, wornAccessories, type LifeStatus, type PanelView, type Settings } from "../api";
import { Pipo } from "../pipo/Pipo";
import Actions from "./actions/Actions";
import Chat from "./Chat";
import Clipboard from "./Clipboard";
import SettingsView from "./Settings";
import SystemView from "./System";
import Today from "./Today";
import Life from "./Life";
import Usage from "./Usage";
import "./panel.css";

const TABS: { view: PanelView; label: string; title?: string }[] = [
  { view: "chat", label: "Chat" },
  { view: "today", label: "Hoje" },
  { view: "actions", label: "Ações" },
  { view: "clipboard", label: "Clipboard", title: "Histórico da área de transferência" },
  { view: "system", label: "Sistema" },
  { view: "usage", label: "Gastos" },
  { view: "settings", label: "⚙", title: "Configurações" },
];

export default function PanelApp() {
  const [view, setView] = useState<PanelView>("chat");
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  // Texto levado para a caixa do chat sem enviar (ex.: "Perguntar ao Pipo" no Clipboard).
  const [draft, setDraft] = useState<string | null>(null);
  const [life, setLife] = useState<LifeStatus | null>(null);

  const refreshKey = () => void api.keyStatus().then(setHasKey);

  useEffect(() => {
    refreshKey();
    void api.getSettings().then(setSettings);
    const unlistenView = listen<PanelView>("panel-view", (e) => {
      setView(e.payload);
      refreshKey();
    });
    const unlistenSettings = listen<Settings>("settings-changed", (e) => setSettings(e.payload));
    const refreshLife = () => void api.lifeStatus().then(setLife);
    refreshLife();
    const unlistenLife = listen("life-changed", refreshLife);
    return () => {
      void unlistenView.then((fn) => fn());
      void unlistenSettings.then((fn) => fn());
      void unlistenLife.then((fn) => fn());
    };
  }, []);

  return (
    <div className="panel">
      <header className="panel-header" data-tauri-drag-region>
        <button
          className="brand"
          title="Nível, evolução e conquistas do Pipo"
          aria-pressed={view === "pipo"}
          onClick={() => setView("pipo")}
        >
          <span className="brand-icon">
            <Pipo state="idle" stage={life?.stage ?? 0} accessories={wornAccessories(life, settings)} />
          </span>
          {life && <span className="brand-level">nv {life.level}</span>}
        </button>
        <nav className="tabs">
          {TABS.map((t) => (
            <button key={t.view} title={t.title} aria-pressed={view === t.view} onClick={() => setView(t.view)}>
              {t.label}
            </button>
          ))}
        </nav>
        <button className="icon-btn" title="Fechar" onClick={() => void api.panelHide()}>
          ✕
        </button>
      </header>

      {view === "pipo" && <Life />}
      {view === "chat" && (
        <Chat
          hasKey={hasKey}
          draft={draft}
          onDraftUsed={() => setDraft(null)}
          onOpenSettings={() => setView("settings")}
        />
      )}
      {view === "today" && <Today onAsk={(prompt) => void api.chatPrefill(prompt, true)} />}
      {view === "actions" && <Actions onAsk={(prompt) => void api.chatPrefill(prompt)} />}
      {view === "clipboard" && (
        <Clipboard
          enabled={settings?.clipboardHistory ?? true}
          onAsk={(text) => {
            setDraft(`Sobre este texto:\n\n${text}\n\n`);
            setView("chat");
          }}
          onRun={(prompt) => void api.chatPrefill(prompt)}
          onOpenSettings={() => setView("settings")}
        />
      )}
      {view === "system" && <SystemView settings={settings} />}
      {view === "usage" && (
        <Usage onOpenConversation={(id) => void api.chatOpen(id).then(() => setView("chat"))} />
      )}
      {view === "settings" && <SettingsView onKeyChange={refreshKey} />}
    </div>
  );
}

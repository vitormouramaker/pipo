import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
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
import { ICONS } from "../ui/icons";
import { sfx } from "../ui/sound";

const TABS: { view: PanelView; icon: keyof typeof ICONS; title: string }[] = [
  { view: "chat", icon: "chat", title: "Chat" },
  { view: "today", icon: "today", title: "Hoje: briefing, lives e pesquisa" },
  { view: "actions", icon: "actions", title: "Ações: dev, foco e atalhos" },
  { view: "clipboard", icon: "clipboard", title: "Área de transferência" },
  { view: "system", icon: "system", title: "Sistema" },
  { view: "usage", icon: "usage", title: "Gastos" },
  { view: "settings", icon: "settings", title: "Configurações" },
];

export default function PanelApp() {
  const [view, setView] = useState<PanelView>("chat");
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  // Texto levado para a caixa do chat sem enviar (ex.: "Perguntar ao Pipo" no Clipboard).
  const [draft, setDraft] = useState<string | null>(null);
  const [life, setLife] = useState<LifeStatus | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [dropping, setDropping] = useState(false);

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "enter" || e.payload.type === "over") setDropping(true);
      else if (e.payload.type === "leave") setDropping(false);
      else if (e.payload.type === "drop") {
        setDropping(false);
        setView("chat");
        void api.chatAttach(e.payload.paths, true);
      }
    });
    return () => void unlisten.then((fn) => fn());
  }, []);

  const refreshKey = () => void api.keyStatus().then(setHasKey);

  useEffect(() => {
    refreshKey();
    void api.getSettings().then(setSettings);
    const unlistenView = listen<PanelView>("panel-view", (e) => {
      setView(e.payload);
      refreshKey();
    });
    const unlistenSettings = listen<Settings>("settings-changed", (e) => setSettings(e.payload));
    void api.panelIsExpanded().then(setExpanded);
    const unlistenExpanded = listen<boolean>("panel-expanded", (e) => setExpanded(e.payload));
    const refreshLife = () => void api.lifeStatus().then(setLife);
    refreshLife();
    const unlistenLife = listen("life-changed", refreshLife);
    return () => {
      void unlistenView.then((fn) => fn());
      void unlistenSettings.then((fn) => fn());
      void unlistenLife.then((fn) => fn());
      void unlistenExpanded.then((fn) => fn());
    };
  }, []);

  return (
    <div className="panel">
      {dropping && (
        <div className="drop-overlay">
          <span>Solte para anexar ao chat</span>
          <small>PDF, Word, PowerPoint, imagem, texto ou código</small>
        </div>
      )}
      <header className="panel-header" data-tauri-drag-region>
        <button
          className="brand"
          title="Nível, evolução e conquistas do Pipo"
          aria-pressed={view === "pipo"}
          onClick={() => {
            if (view !== "pipo") sfx.tick();
            setView("pipo");
          }}
        >
          <span className="brand-icon">
            <Pipo state="idle" stage={life?.stage ?? 0} accessories={wornAccessories(life, settings)} />
          </span>
          {life && <span className="brand-level">nv {life.level}</span>}
        </button>
        <nav className="tabs">
          {TABS.map((t) => (
            <button
              key={t.view}
              className="tab-icon"
              title={t.title}
              aria-label={t.title}
              aria-pressed={view === t.view}
              onClick={() => {
                if (view !== t.view) sfx.tick();
                setView(t.view);
              }}
            >
              {ICONS[t.icon]}
            </button>
          ))}
        </nav>
        <button
          className="icon-btn tab-icon"
          title={expanded ? "Voltar ao tamanho normal" : "Expandir para a tela toda"}
          aria-label={expanded ? "Voltar ao tamanho normal" : "Expandir para a tela toda"}
          onClick={() => {
            sfx.tick();
            void api.panelExpand().then(setExpanded);
          }}
        >
          {expanded ? ICONS.collapse : ICONS.expand}
        </button>
        <button className="icon-btn tab-icon" title="Fechar" aria-label="Fechar" onClick={() => void api.panelHide()}>
          {ICONS.close}
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

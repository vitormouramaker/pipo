import { Channel, invoke } from "@tauri-apps/api/core";

export type Settings = {
  model: string;
  sarcasm: number;
  shortcut: string;
  petSize: number;
  autostart: boolean;
};

export type ChatMessage = { role: "user" | "assistant"; text: string };

export type ChatEvent =
  | { type: "delta"; text: string }
  | { type: "done"; stopReason: string | null }
  | { type: "error"; message: string };

export type PanelView = "chat" | "settings";

export const api = {
  chatHistory: () => invoke<ChatMessage[]>("chat_history"),
  chatNew: () => invoke<void>("chat_new"),
  chatSend: (text: string, onEvent: (e: ChatEvent) => void) => {
    const channel = new Channel<ChatEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("chat_send", { text, onEvent: channel });
  },
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  keyStatus: () => invoke<boolean>("key_status"),
  saveApiKey: (key: string) => invoke<void>("save_api_key", { key }),
  deleteApiKey: () => invoke<void>("delete_api_key"),
  importApiKeyFile: () => invoke<boolean>("import_api_key_file"),
  panelOpen: (view: PanelView) => invoke<void>("panel_open", { view }),
  panelToggle: () => invoke<void>("panel_toggle"),
  panelHide: () => invoke<void>("panel_hide"),
  petToggle: () => invoke<void>("pet_toggle"),
  quit: () => invoke<void>("app_quit"),
};

export const MODELS = [
  { id: "claude-haiku-5-5", label: "Claude Haiku 5.5 — rápido e econômico" },
  { id: "claude-sonnet-5-5", label: "Claude Sonnet 5.5 — equilibrado" },
  { id: "claude-opus-5-5", label: "Claude Opus 5.5 — mais capaz" },
];

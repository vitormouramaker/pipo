import { Channel, invoke } from "@tauri-apps/api/core";

export type Settings = {
  model: string;
  sarcasm: number;
  shortcut: string;
  petSize: number;
  autostart: boolean;
  senseContext: boolean;
  gameMode: boolean;
  clipboardHistory: boolean;
  hardwareAlerts: boolean;
  sleepWhenIdle: boolean;
};

export type ChatMessage = { role: "user" | "assistant"; text: string };

export type Conversation = { id: number; title: string; updatedAt: string; current: boolean };

export type ChatEvent =
  | { type: "delta"; text: string }
  | { type: "done"; stopReason: string | null }
  | { type: "error"; message: string };

export type PanelView = "chat" | "clipboard" | "system" | "usage" | "settings";

export type UsageSummary = {
  today: number;
  month: number;
  total: number;
  conversations: {
    id: number;
    title: string;
    updatedAt: string;
    requests: number;
    inputTokens: number;
    outputTokens: number;
    cost: number;
  }[];
  models: { model: string; requests: number; cost: number }[];
};

export type ClipItem = { id: number; text: string; at: string };

export type ProcessUsage = { name: string; cpu: number; memory: number };

export type HardwareInfo = {
  cpu: number;
  cores: number;
  memUsed: number;
  memTotal: number;
  gpu: { name: string; usage: number; memUsed: number; memTotal: number; temperature: number | null } | null;
  topCpu: ProcessUsage[];
  topMemory: ProcessUsage[];
};

export type Category = "dev" | "docs" | "browser" | "live" | "games" | "chat" | "other";

export type SenseStatus = {
  context: {
    exe: string;
    name: string;
    title: string;
    category: Category;
    project: string | null;
    fullscreen: boolean;
  } | null;
  mood: string;
  gaming: boolean;
  idleSeconds: number;
};

export type RadialLayout = {
  petSize: number;
  margin: number;
  radius: number;
  angles: number[];
  suggestion: { label: string; prompt: string } | null;
};

export const api = {
  chatHistory: () => invoke<ChatMessage[]>("chat_history"),
  chatNew: () => invoke<void>("chat_new"),
  chatConversations: () => invoke<Conversation[]>("chat_conversations"),
  chatOpen: (id: number) => invoke<ChatMessage[]>("chat_open", { id }),
  chatDelete: (id: number) => invoke<void>("chat_delete", { id }),
  chatPrefill: (text: string) => invoke<void>("chat_prefill", { text }),
  chatTakePrefill: () => invoke<string | null>("chat_take_prefill"),
  chatSend: (text: string, onEvent: (e: ChatEvent) => void) => {
    const channel = new Channel<ChatEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("chat_send", { text, onEvent: channel });
  },
  usageSummary: () => invoke<UsageSummary>("usage_summary"),
  clipboardList: () => invoke<ClipItem[]>("clipboard_list"),
  clipboardCopy: (id: number) => invoke<void>("clipboard_copy", { id }),
  clipboardRemove: (id: number) => invoke<void>("clipboard_remove", { id }),
  clipboardClear: () => invoke<void>("clipboard_clear"),
  systemStats: () => invoke<HardwareInfo>("system_stats"),
  systemRelease: () => invoke<void>("system_release"),
  senseStatus: () => invoke<SenseStatus>("sense_status"),
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
  petDragStart: () => invoke<void>("pet_drag_start"),
  radialOpen: () => invoke<RadialLayout>("radial_open"),
  radialClose: () => invoke<void>("radial_close"),
  quit: () => invoke<void>("app_quit"),
};

export const MODELS = [
  { id: "claude-haiku-5-5", label: "Claude Haiku 5.5 — rápido e econômico" },
  { id: "claude-sonnet-5-5", label: "Claude Sonnet 5.5 — equilibrado" },
  { id: "claude-opus-5-5", label: "Claude Opus 5.5 — mais capaz" },
];

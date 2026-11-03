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
  breakReminders: boolean;
  breakIntervalMin: number;
  webSearch: boolean;
  briefingEnabled: boolean;
  briefingTime: string;
  briefingTopics: string;
  briefingCity: string;
};

export type Source = { title: string; url: string };
export type Briefing = { day: string; content: string; sources: Source[]; createdAt: string };
export type LiveChannel = {
  key: string;
  platform: "twitch" | "youtube";
  handle: string;
  url: string;
  live: boolean | null;
  title: string | null;
  checkedAt: string | null;
  error: string | null;
};

export type Project = { name: string; path: string; editor: string };
export type ProjectInfo = {
  runner: string | null;
  scripts: { name: string; detail: string; command: string }[];
  git: boolean;
};
export type Job = {
  id: number;
  project: string;
  command: string;
  status: "running" | "ok" | "failed" | "stopped";
  exitCode: number | null;
  startedAt: string;
  durationSecs: number | null;
  output: string[];
};
export type GitChanges = { staged: boolean; status: string; diff: string; truncated: boolean };
export type Port = { port: number; pid: number; process: string; address: string };
export type Container = { id: string; name: string; image: string; state: string; status: string; ports: string };
export type Game = { id: string; name: string; launcher: string; favorite: boolean };
export type RoutineStep = { kind: "app" | "url" | "folder" | "project"; target: string };
export type Routine = { id: string; name: string; steps: RoutineStep[] };
export type RecentFile = { name: string; path: string; folder: boolean; modified: string };
export type Note = { id: number; text: string; createdAt: string };
export type FocusStatus = {
  pomodoro: { phase: "focus" | "break"; remainingSecs: number; cycle: number; focusMin: number; breakMin: number } | null;
  reminders: { id: number; text: string; due: string }[];
  breaksEnabled: boolean;
  breakIntervalMin: number;
  nextBreakSecs: number | null;
};

export type ChatMessage = {
  role: "user" | "assistant" | "tool" | "confirm";
  text: string;
  confirmId?: string;
  decision?: "approved" | "denied";
  sources?: { title: string; url: string }[];
};

export type Conversation = { id: number; title: string; updatedAt: string; current: boolean };

export type ChatEvent =
  | { type: "delta"; text: string }
  | { type: "tool"; label: string }
  | { type: "confirm"; id: string; question: string }
  | { type: "source"; title: string; url: string }
  | { type: "done"; stopReason: string | null }
  | { type: "error"; message: string };

export type PanelView = "chat" | "today" | "actions" | "clipboard" | "system" | "usage" | "settings";

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
  chatConfirm: (id: string, approved: boolean) => invoke<void>("chat_confirm", { id, approved }),
  projectsRecent: () => invoke<Project[]>("projects_recent"),
  projectDetails: (path: string) => invoke<ProjectInfo>("project_details", { path }),
  projectOpen: (path: string) => invoke<void>("project_open", { path }),
  projectTerminal: (path: string, script?: string) => invoke<void>("project_terminal", { path, script }),
  jobsList: () => invoke<Job[]>("jobs_list"),
  jobStart: (path: string, script: string) => invoke<number>("job_start", { path, script }),
  jobStop: (id: number) => invoke<void>("job_stop", { id }),
  jobsClear: () => invoke<void>("jobs_clear"),
  gitChanges: (path: string) => invoke<GitChanges>("git_changes", { path }),
  portsList: () => invoke<Port[]>("ports_list"),
  processKill: (pid: number) => invoke<void>("process_kill", { pid }),
  dockerList: () => invoke<Container[]>("docker_list"),
  dockerAction: (id: string, action: "start" | "stop" | "restart") => invoke<void>("docker_action", { id, action }),
  gamesList: () => invoke<Game[]>("games_list"),
  gameLaunch: (id: string) => invoke<void>("game_launch", { id }),
  gameFavorite: (id: string, favorite: boolean) => invoke<void>("game_favorite", { id, favorite }),
  routinesList: () => invoke<Routine[]>("routines_list"),
  routinesSave: (routines: Routine[]) => invoke<void>("routines_save", { routines }),
  routineRun: (id: string) => invoke<string[]>("routine_run", { id }),
  recentFiles: (query?: string) => invoke<RecentFile[]>("recent_files", { query }),
  recentOpen: (path: string) => invoke<void>("recent_open", { path }),
  notesList: (query?: string) => invoke<Note[]>("notes_list", { query }),
  noteAdd: (text: string) => invoke<number>("note_add", { text }),
  noteDelete: (id: number) => invoke<void>("note_delete", { id }),
  focusStatus: () => invoke<FocusStatus>("focus_status"),
  reminderAdd: (text: string, minutes?: number, at?: string) => invoke<string>("reminder_add", { text, minutes, at }),
  reminderDelete: (id: number) => invoke<void>("reminder_delete", { id }),
  pomodoroStart: (focusMin: number, breakMin: number) => invoke<void>("pomodoro_start", { focusMin, breakMin }),
  pomodoroStop: () => invoke<void>("pomodoro_stop"),
  ocrStart: () => invoke<void>("ocr_start"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  livesList: () => invoke<LiveChannel[]>("lives_list"),
  liveAdd: (platform: string, input: string) => invoke<void>("live_add", { platform, input }),
  liveRemove: (key: string) => invoke<void>("live_remove", { key }),
  livesRefresh: () => invoke<void>("lives_refresh"),
  briefingLatest: () => invoke<Briefing | null>("briefing_latest"),
  briefingGenerate: () => invoke<Briefing>("briefing_generate"),
  briefingRunning: () => invoke<boolean>("briefing_running"),
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

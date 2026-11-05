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
  city: string;
  voiceModel: "base" | "small";
  voiceShortcut: string;
  screenShortcut: string;
  findShortcut: string;
  plainPasteShortcut: string;
  captionsShortcut: string;
  captionsTranslate: boolean;
  captionsOriginal: boolean;
  captionsSize: number;
  speakReplies: boolean;
  voiceName: string;
  voiceEngine: "natural" | "windows";
  naturalVoice: "faber" | "cadu";
  followCursor: boolean;
  hiddenAccessories: string[];
  skin: string;
  islandStyle: string;
  dance: boolean;
  personality: string;
  replyLength: string;
  recentContext: boolean;
  sounds: boolean;
  soundVolume: number;
  proactive: boolean;
  daySummaryTime: string;
  calendarUrl: string;
  githubNotify: boolean;
  island: boolean;
  islandEdge: string;
  islandPos: number;
  autoModes: boolean;
  manualMode: string;
  modeRules: ModeRule[];
  modeAllow: Record<string, string[]>;
  textExpander: boolean;
  snippets: Snippet[];
  lolEnabled: boolean;
  lolTips: boolean;
  lolWebBuild: boolean;
  lolTiltAlert: boolean;
  telegramEnabled: boolean;
  telegramBot: string;
  telegramChatId: string;
  telegramAwayOnly: boolean;
  telegramSources: string[];
  telegramCommands: boolean;
  telegramRemindersAlways: boolean;
  weeklyReport: boolean;
  weeklyReportDay: number;
  weeklyReportTime: string;
  weeklyReportAi: boolean;
  automations: Automation[];
  fileIndex: boolean;
  cloudFolder: string;
  cloudAuto: boolean;
  cloudMode: string;
  gdriveClientId: string;
  googleScopes: string;
  googleCalendar: boolean;
  calendarRemindMinutes: number;
  calendarLeaveMinutes: number;
  emailEnabled: boolean;
  emailAddress: string;
  emailNotify: boolean;
};

export type LifeStatus = {
  totalXp: number;
  level: number;
  levelXp: number;
  levelNeed: number;
  stage: number;
  stageName: string;
  stages: { name: string; fromLevel: number; detail: string }[];
  accessories: { id: string; name: string; level: number; unlocked: boolean }[];
  achievements: { id: string; title: string; detail: string; unlockedAt: string | null; progress: number; goal: number }[];
  streak: number;
  days: number;
  since: string | null;
  chats: number;
  actions: number;
  pomodoros: number;
  searches: number;
  voice: number;
};

export type VoiceStatus = {
  model: string;
  modelReady: boolean;
  downloading: boolean;
  downloaded: number;
  total: number;
  listening: boolean;
  hasMicrophone: boolean;
};
export type Toast = { id: number; kind: string; title: string; body: string; url: string | null; view: string | null; prompt: string | null; mini: string | null };
export type DaySummary = {
  day: string;
  activeSecs: number;
  categories: [string, number][];
  apps: [string, string, number][];
  pomodoros: number;
  chats: number;
  actions: number;
};
export type Memory = { id: number; text: string; createdAt: string };
export type CalendarEvent = { id: string; title: string; start: string; end: string; time: string; allDay: boolean; location: string; link: string | null };
export type CalendarStatus = {
  configured: boolean;
  google: boolean;
  today: CalendarEvent[];
  tomorrow: CalendarEvent[];
  error: string | null;
  checkedAt: string | null;
};
export type Pr = { account: string; repo: string; number: number; title: string; url: string };
export type CiRun = { repo: string; id: number; status: string; conclusion: string; title: string; url: string; branch: string; workflow: string };
export type GithubStatus = { available: boolean; login: string | null; accounts: string[]; prs: Pr[]; reviews: Pr[]; runs: CiRun[]; error: string | null; checkedAt: string | null };
export type NowPlaying = { available: boolean; title: string; artist: string; app: string; playing: boolean };
export type UpdateStatus = {
  current: string;
  available: string | null;
  notes: string | null;
  checking: boolean;
  installing: boolean;
  error: string | null;
};
export type Attachment = { path: string; name: string; kind: "pdf" | "image" | "docx" | "pptx" | "text"; size: number };
export type AttachResult = { path: string; name: string; attachment: Attachment | null; error: string | null };
export type AutomationTrigger = {
  kind: "app_open" | "app_close" | "time" | "mode_on" | "mode_off" | "back" | "startup";
  value: string;
  at: string;
  days: number[];
};
export type AutomationAction = {
  kind: "open" | "routine" | "notify" | "telegram" | "mode" | "pomodoro" | "media" | "island" | "wait";
  value: string;
};
export type Automation = { id: string; name: string; enabled: boolean; trigger: AutomationTrigger; actions: AutomationAction[] };
export type OldCopy = { path: string; version: string };
export type FileHit = { path: string; name: string; folder: string; snippet: string; modified: string };
export type FileIndexStatus = { files: number; withContent: number; running: boolean; progress: number; updatedAt: string | null; error: string | null };
export type CloudBackup = { path: string; pc: string; when: string; sizeKb: number; thisPc: boolean };
export type DriveBackup = { id: string; pc: string; when: string; sizeKb: number; thisPc: boolean };
export type CloudStatus = {
  driveConnected: boolean;
  driveBackups: DriveBackup[];
  driveError: string | null;
  detected: string[];
  folder: string;
  lastBackup: string | null;
  backups: CloudBackup[];
  pc: string;
};
export type WeeklyReport = { week: string; text: string; createdAt: string };
export type MiniSource = { platform: string; handle: string; title: string | null; video: string | null; url: string };
export type ModeStatus = { mode: string; reason: string };
export type ModeRule = { mode: string; kind: "app" | "horario"; apps: string; days: number[]; from: string; to: string };
export type Snippet = { trigger: string; text: string };
export type LolStatus = {
  running: boolean;
  phase: string;
  champion: string | null;
  position: string | null;
  tip: string | null;
  lastGame: string | null;
  championIcon: LolIcon | null;
  enemies: LolIcon[];
  build: LolBuild | null;
};
export type LolIcon = { name: string; url: string };
export type LolBuild = {
  patch: string;
  keystone: LolIcon | null;
  primary: LolIcon[];
  secondary: LolIcon[];
  summoners: LolIcon[];
  starting: LolIcon[];
  core: LolIcon[];
  boots: LolIcon | null;
  situational: LolIcon[];
  tips: string[];
};
export type LolGame = {
  icon: string | null;
  day: string;
  champion: string;
  position: string;
  won: boolean;
  kills: number;
  deaths: number;
  assists: number;
  csMin: number;
  minutes: number;
};
export type LolHistory = { games: LolGame[]; wins: number; top: [string, number][]; kda: number; csMin: number };
export type TelegramStatus = {
  hasToken: boolean;
  linked: boolean;
  bot: string;
  listening: boolean;
  error: string | null;
  lastMessage: string | null;
};
export type Mail = { uid: number; from: string; subject: string; date: string; snippet: string };
export type FullMail = { uid: number; from: string; replyTo: string; subject: string; date: string; body: string };
export type MailStatus = { configured: boolean; address: string; unread: Mail[]; checkedAt: string | null; error: string | null };
export type DownloadsPlan = {
  loose: number;
  groups: { folder: string; count: number; bytes: number }[];
  candidates: { path: string; name: string; reason: string; bytes: number }[];
  canUndo: boolean;
};
export type VoiceState = { phase: "idle" | "listening" | "transcribing" | "error" | "nomodel"; message: string | null };

/** Acessórios em uso: os liberados que o usuário não escondeu. */
export const wornAccessories = (life: LifeStatus | null, settings: Settings | null) =>
  (life?.accessories ?? []).filter((a) => a.unlocked && !settings?.hiddenAccessories.includes(a.id)).map((a) => a.id);

export type Source = { title: string; url: string };
export type Briefing = { day: string; content: string; sources: Source[]; createdAt: string };
export type LiveChannel = {
  key: string;
  platform: "twitch" | "youtube" | "kick";
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
  reminders: { id: number; text: string; due: string; repeat: string }[];
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
  images?: string[];
};

export type Conversation = { id: number; title: string; updatedAt: string; current: boolean };

export type ChatEvent =
  | { type: "delta"; text: string }
  | { type: "tool"; label: string }
  | { type: "confirm"; id: string; question: string }
  | { type: "source"; title: string; url: string }
  | { type: "done"; stopReason: string | null }
  | { type: "error"; message: string };

export type PanelView = "pipo" | "chat" | "today" | "actions" | "clipboard" | "system" | "usage" | "settings";

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

export type ClipItem = { id: number; kind: "text" | "image"; text: string; at: string; pinned: boolean; thumb?: string };

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

export type Category = "dev" | "docs" | "browser" | "live" | "games" | "chat" | "ai" | "other";

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
  chatPrefill: (text: string, fresh = false) => invoke<void>("chat_prefill", { text, fresh }),
  chatTakePrefill: () => invoke<{ text: string; fresh: boolean; voice: boolean } | null>("chat_take_prefill"),
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
  reminderAdd: (text: string, minutes?: number, at?: string, repeat?: string) => invoke<string>("reminder_add", { text, minutes, at, repeat }),
  reminderDelete: (id: number) => invoke<void>("reminder_delete", { id }),
  pomodoroStart: (focusMin: number, breakMin: number) => invoke<void>("pomodoro_start", { focusMin, breakMin }),
  pomodoroStop: () => invoke<void>("pomodoro_stop"),
  ocrStart: () => invoke<void>("ocr_start"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  detectCity: () => invoke<string>("detect_city"),
  livesList: () => invoke<LiveChannel[]>("lives_list"),
  liveAdd: (input: string) => invoke<string>("live_add", { input }),
  liveRemove: (key: string) => invoke<void>("live_remove", { key }),
  livesRefresh: () => invoke<void>("lives_refresh"),
  briefingLatest: () => invoke<Briefing | null>("briefing_latest"),
  briefingGenerate: () => invoke<Briefing>("briefing_generate"),
  briefingRunning: () => invoke<boolean>("briefing_running"),
  chatSend: (text: string, onEvent: (e: ChatEvent) => void, attachments: string[] = []) => {
    const channel = new Channel<ChatEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("chat_send", { text, attachments, onEvent: channel });
  },
  chatAttach: (paths: string[], openChat = true) => invoke<AttachResult[]>("chat_attach", { paths, openChat }),
  chatTakeAttachments: () => invoke<Attachment[]>("chat_take_attachments"),
  attachmentPreview: (path: string) => invoke<string>("attachment_preview", { path }),
  chatAttachmentsCount: (count: number) => invoke<void>("chat_attachments_count", { count }),
  usageSummary: () => invoke<UsageSummary>("usage_summary"),
  clipboardList: () => invoke<ClipItem[]>("clipboard_list"),
  clipboardCopy: (id: number) => invoke<void>("clipboard_copy", { id }),
  clipboardRemove: (id: number) => invoke<void>("clipboard_remove", { id }),
  clipboardClear: () => invoke<void>("clipboard_clear"),
  clipboardPin: (id: number, pinned: boolean) => invoke<void>("clipboard_pin", { id, pinned }),
  clipboardAskImage: (id: number) => invoke<void>("clipboard_ask_image", { id }),
  systemStats: () => invoke<HardwareInfo>("system_stats"),
  systemRelease: () => invoke<void>("system_release"),
  senseStatus: () => invoke<SenseStatus>("sense_status"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  /** Salva só os campos alterados (sem sobrescrever o que outra tela mudou). */
  settingsUpdate: (patch: Partial<Settings>) => invoke<Settings>("settings_update", { patch }),
  keyStatus: () => invoke<boolean>("key_status"),
  saveApiKey: (key: string) => invoke<void>("save_api_key", { key }),
  deleteApiKey: () => invoke<void>("delete_api_key"),
  importApiKeyFile: () => invoke<boolean>("import_api_key_file"),
  panelOpen: (view: PanelView) => invoke<void>("panel_open", { view }),
  panelToggle: () => invoke<void>("panel_toggle"),
  panelHide: () => invoke<void>("panel_hide"),
  panelExpand: () => invoke<boolean>("panel_expand"),
  panelIsExpanded: () => invoke<boolean>("panel_is_expanded"),
  petToggle: () => invoke<void>("pet_toggle"),
  petExpress: (state: "speaking" | "clear") => invoke<void>("pet_express", { state }),
  lifeStatus: () => invoke<LifeStatus>("life_status"),
  voiceStatus: (model?: string) => invoke<VoiceStatus>("voice_status", { model }),
  voiceDownload: (model?: string) => invoke<void>("voice_download", { model }),
  voiceToggle: () => invoke<void>("voice_toggle"),
  screenAsk: () => invoke<void>("screen_ask"),
  petFind: () => invoke<void>("pet_find"),
  captionsToggle: () => invoke<void>("captions_toggle"),
  captionsStatus: () => invoke<boolean>("captions_status"),
  downloadsPlan: () => invoke<DownloadsPlan>("downloads_plan"),
  downloadsOrganize: () => invoke<number>("downloads_organize"),
  downloadsUndo: () => invoke<number>("downloads_undo"),
  downloadsTrash: (paths: string[]) => invoke<number>("downloads_trash", { paths }),
  ttsStatus: () => invoke<{ ready: boolean; downloading: boolean; downloaded: number; total: number }>("tts_status"),
  ttsDownload: () => invoke<void>("tts_download"),
  ttsSynth: (text: string) => invoke<string>("tts_synth", { text }),
  speechStop: () => invoke<void>("speech_stop"),
  petMenuOpen: () => invoke<void>("pet_menu_open"),
  menuPending: () => invoke<boolean | null>("menu_pending"),
  menuLayout: (width: number, height: number) => invoke<void>("menu_layout", { width, height }),
  menuHide: () => invoke<void>("menu_hide"),
  islandToggle: () => invoke<void>("island_toggle"),
  updateStatus: () => invoke<UpdateStatus>("update_status"),
  updateCheck: () => invoke<UpdateStatus>("update_check"),
  updateInstall: () => invoke<void>("update_install"),
  islandStatus: () => invoke<{ active: boolean; edge: string; scale: number }>("island_status"),
  toastTake: () => invoke<Toast[]>("toast_take"),
  toastLayout: (height: number) => invoke<void>("toast_layout", { height }),
  toastOpen: (url: string | null, view: string | null, prompt: string | null, mini: string | null) =>
    invoke<void>("toast_open", { url, view, prompt, mini }),
  miniOpen: (key: string) => invoke<void>("mini_open", { key }),
  miniSource: () => invoke<MiniSource | null>("mini_source"),
  miniClose: () => invoke<void>("mini_close"),
  modeStatus: () => invoke<ModeStatus>("mode_status"),
  lolStatus: () => invoke<LolStatus>("lol_status"),
  lolHistory: () => invoke<LolHistory>("lol_history"),
  telegramStatus: () => invoke<TelegramStatus>("telegram_status"),
  telegramConnect: (token: string) => invoke<string>("telegram_connect", { token }),
  telegramLink: () => invoke<string>("telegram_link"),
  telegramTest: () => invoke<void>("telegram_test"),
  telegramDisconnect: () => invoke<void>("telegram_disconnect"),
  emailStatus: () => invoke<MailStatus>("email_status"),
  reportLatest: () => invoke<WeeklyReport | null>("report_latest"),
  cloudStatus: () => invoke<CloudStatus>("cloud_status"),
  cloudBackup: () => invoke<string>("cloud_backup"),
  cloudRestore: (path: string) => invoke<void>("cloud_restore", { path }),
  gdriveConnect: (clientId: string, clientSecret: string) => invoke<void>("gdrive_connect", { clientId, clientSecret }),
  gdriveDisconnect: () => invoke<void>("gdrive_disconnect"),
  gdriveRestore: (id: string) => invoke<void>("gdrive_restore", { id }),
  filesSearch: (query: string) => invoke<FileHit[]>("files_search", { query }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  oldCopies: () => invoke<OldCopy[]>("old_copies"),
  oldCopyRemove: (path: string) => invoke<void>("old_copy_remove", { path }),
  filesIndexStatus: () => invoke<FileIndexStatus>("files_index_status"),
  filesReindex: () => invoke<void>("files_reindex"),
  automationTest: (automation: Automation) => invoke<string[]>("automation_test", { automation }),
  runningPrograms: () => invoke<string[]>("running_programs"),
  reportGenerate: () => invoke<WeeklyReport>("report_generate"),
  reportSend: () => invoke<void>("report_send"),
  emailRefresh: () => invoke<MailStatus>("email_refresh"),
  emailConnect: (address: string, password: string) => invoke<number>("email_connect", { address, password }),
  emailDisconnect: () => invoke<void>("email_disconnect"),
  emailRead: (uid: number) => invoke<FullMail>("email_read", { uid }),
  emailDraft: (uid: number, instructions: string) => invoke<string>("email_draft", { uid, instructions }),
  emailSend: (uid: number, body: string) => invoke<string>("email_send", { uid, body }),
  emailSaveDraft: (uid: number, body: string) => invoke<string>("email_save_draft", { uid, body }),
  daySummary: () => invoke<DaySummary>("day_summary"),
  calendarStatus: () => invoke<CalendarStatus>("calendar_status"),
  calendarRefresh: () => invoke<number>("calendar_refresh"),
  googleCalendarConnect: (clientId: string, clientSecret: string) => invoke<number>("google_calendar_connect", { clientId, clientSecret }),
  githubStatus: () => invoke<GithubStatus>("github_status"),
  githubRefresh: () => invoke<GithubStatus>("github_refresh"),
  mediaStatus: () => invoke<NowPlaying>("media_status"),
  mediaControl: (action: "toggle" | "next" | "previous") => invoke<NowPlaying>("media_control", { action }),
  memoryList: () => invoke<Memory[]>("memory_list"),
  memoryRemove: (id: number) => invoke<void>("memory_remove", { id }),
  toastDone: () => invoke<void>("toast_done"),
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

export type ConnectionState = "disconnected" | "connecting" | "connected";
export type FadeState = "idle" | "running" | "blocked";
export type LogSource = "app" | "lv1" | "fade";
export type LogSeverity = "info" | "warning" | "error";

export type SceneSummary = {
  index: number;
  name: string;
};

export type ChannelSummary = {
  group: number;
  channel: number;
  name: string;
};

export type ChannelRef = {
  group: number;
  channel: number;
};

export type ChannelConfig = {
  group: number;
  channel: number;
  faderDb: number | null;
};

export type SceneConfig = {
  sceneId: string;
  sceneIndex: number;
  sceneName: string;
  durationMs: number;
  channelConfigs: ChannelConfig[];
  scopedChannels: ChannelRef[];
};

export type AppLogEntry = {
  id: number;
  timestamp: string;
  source: LogSource;
  severity: LogSeverity;
  message: string;
};

export type AppViewState = {
  connection: ConnectionState;
  currentScene: SceneSummary | null;
  scenes: SceneSummary[];
  sceneCount: number;
  channelCount: number;
  channels: ChannelSummary[];
  fadeState: FadeState;
  lockout: boolean;
  logs: AppLogEntry[];
  lastEventAt: string | null;
  sceneConfigs: SceneConfig[];
  selectedSceneId: string | null;
  showFileName: string;
  showFilePath: string | null;
  showFileDirty: boolean;
  showFileLastSavedAt: string | null;
};

export const disconnectedAppViewState: AppViewState = {
  connection: "disconnected",
  currentScene: null,
  scenes: [],
  sceneCount: 0,
  channelCount: 0,
  channels: [],
  fadeState: "idle",
  lockout: false,
  logs: [],
  lastEventAt: null,
  sceneConfigs: [],
  selectedSceneId: null,
  showFileName: "Untitled Show",
  showFilePath: null,
  showFileDirty: false,
  showFileLastSavedAt: null,
};

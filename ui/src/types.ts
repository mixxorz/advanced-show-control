export type ConnectionState = "disconnected" | "connecting" | "connected";
export type DiscoveredLv1Status = "available" | "connecting" | "connected" | "unavailable";
export type FadeState = "idle" | "running" | "blocked";
export type LogSource = "app" | "lv1" | "fade";
export type LogSeverity = "info" | "warning" | "error";

export type Lv1SystemIdentity = {
  uuid: string | null;
  host: string | null;
  address: string;
  port: number;
};

export type DiscoveredLv1System = {
  identity: Lv1SystemIdentity;
  latencyMs: number | null;
  status: DiscoveredLv1Status;
};

export type ReconnectState = {
  active: boolean;
  attempt: number;
};

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
  discoveredLv1Systems: DiscoveredLv1System[];
  connectedLv1Identity: Lv1SystemIdentity | null;
  pendingLv1Identity: Lv1SystemIdentity | null;
  reconnect: ReconnectState;
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
  discoveredLv1Systems: [],
  connectedLv1Identity: null,
  pendingLv1Identity: null,
  reconnect: { active: false, attempt: 0 },
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

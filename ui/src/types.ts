export type ConnectionState = "disconnected" | "connecting" | "connected";
export type FadeState = "idle" | "running" | "blocked";
export type LogSource = "app" | "lv1" | "fade";
export type LogSeverity = "info" | "warning" | "error";

export type SceneSummary = {
  index: number;
  name: string;
};

export type AppLogEntry = {
  id: number;
  timestamp: string;
  source: LogSource;
  severity: LogSeverity;
  message: string;
};

export type AppSnapshot = {
  connection: ConnectionState;
  currentScene: SceneSummary | null;
  scenes: SceneSummary[];
  sceneCount: number;
  channelCount: number;
  fadeState: FadeState;
  lockout: boolean;
  logs: AppLogEntry[];
  lastEventAt: string | null;
};

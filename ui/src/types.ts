export type ConnectionState = "disconnected" | "connecting" | "connected";
export type DiscoveredLv1Status =
  | "available"
  | "connecting"
  | "connected"
  | "unavailable";
export type FadeState = "idle" | "running" | "blocked";
export type LogSeverity = "info" | "warning" | "error";
export type PanMode = "none" | "mono" | "stereo";

export type TimeDisplayFormat = "twelveHour" | "twentyFourHour";

export type KeyboardShortcutModifiers = {
  shift: boolean;
  control: boolean;
  alt: boolean;
  meta: boolean;
};

export type KeyboardShortcut = {
  key: string;
  modifiers: KeyboardShortcutModifiers;
};

export type KeyboardShortcutSettings = {
  go: KeyboardShortcut;
  cue: KeyboardShortcut;
};

/**
 * @cc [owner:mixxorz,label:architecture] complete-settings-value
 * `AppSettings` MUST remain a complete replacement value matching backend settings serialization;
 * frontend edits MUST preserve and submit fields they do not change.
 */
export type AppSettings = {
  autoLoadLastShowFile: boolean;
  autoSaveSessions: boolean;
  keyboardShortcuts: KeyboardShortcutSettings;
  timeDisplay: TimeDisplayFormat;
  faderOverrideSensitivity: number;
  enableExtensiveDiagnostics: boolean;
  sameSceneRecallEnabled: boolean;
  sameSceneRecallThresholdMs: number;
};

export type TcpConnectLatencyResult = {
  tcpConnectMs: number;
};

export type Lv1SystemIdentity = {
  uuid: string | null;
  host: string | null;
  address: string;
  port: number;
};

export type DiscoveredLv1System = {
  identity: Lv1SystemIdentity;
  status: DiscoveredLv1Status;
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
  pan: number | null;
  balance: number | null;
  width: number | null;
  panMode: PanMode | null;
};

export type SceneScopeToggles = {
  faders: boolean;
  pan: boolean;
};

export type SceneConfig = {
  internalSceneId: string;
  sceneIndex: number | null;
  sceneName: string;
  durationMs: number;
  scopeToggles: SceneScopeToggles;
  channelConfigs: ChannelConfig[];
  scopedChannels: ChannelRef[];
};

export type CueEntry = {
  id: string;
  sceneInternalId: string;
};

export type CueList = {
  id: string;
  name: string;
  entries: CueEntry[];
};

export type AppLogEntry = {
  id: number;
  timestamp: string;
  severity: LogSeverity;
  message: string;
};

/**
 * @cc [owner:mixxorz,label:architecture;api] serialized-view-mirror
 * `AppViewState` MUST mirror the camel-cased Tauri payload defined by
 * `src-tauri/src/projector/view.rs`; frontend-only fields MUST NOT be added to create a second owner
 * for backend state.
 */
export type AppViewState = {
  settings: AppSettings;
  connection: ConnectionState;
  discoveredLv1Systems: DiscoveredLv1System[];
  connectedLv1Identity: Lv1SystemIdentity | null;
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
  sceneSettingsClipboardAvailable: boolean;
  selectedSceneInternalId: string | null;
  cueLists: CueList[];
  activeCueListId: string | null;
  cuedCueEntryId: string | null;
  lastCueRecallStatus: string | null;
  showFileName: string;
  showFilePath: string | null;
  showFileDirty: boolean;
  showFileLastSavedAt: string | null;
  stateVersion: number;
};

/**
 * @cc [owner:mixxorz,label:architecture] pre-snapshot-placeholder
 * This value MUST be used only before the first accepted backend snapshot or in isolated mocks; it
 * MUST NOT overwrite an accepted snapshot or be treated as evidence of backend disconnection.
 */
export const disconnectedAppViewState: AppViewState = {
  settings: {
    autoLoadLastShowFile: false,
    autoSaveSessions: false,
    keyboardShortcuts: {
      go: {
        key: "Space",
        modifiers: { shift: false, control: false, alt: false, meta: false },
      },
      cue: {
        key: "C",
        modifiers: { shift: false, control: false, alt: false, meta: false },
      },
    },
    timeDisplay: "twentyFourHour",
    faderOverrideSensitivity: 9,
    enableExtensiveDiagnostics: false,
    sameSceneRecallEnabled: true,
    sameSceneRecallThresholdMs: 500,
  },
  connection: "disconnected",
  discoveredLv1Systems: [],
  connectedLv1Identity: null,
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
  sceneSettingsClipboardAvailable: false,
  selectedSceneInternalId: null,
  cueLists: [],
  activeCueListId: null,
  cuedCueEntryId: null,
  lastCueRecallStatus: null,
  showFileName: "Untitled Session",
  showFilePath: null,
  showFileDirty: false,
  showFileLastSavedAt: null,
  stateVersion: 0,
};

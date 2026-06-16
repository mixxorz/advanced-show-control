import { disconnectedAppViewState, type AppLogEntry, type AppViewState, type SceneConfig } from "../types";

const channels = [
  { group: 0, channel: 0, name: "Lead Vocal" },
  { group: 0, channel: 1, name: "B Vocal" },
  { group: 0, channel: 2, name: "Guitar" },
  { group: 0, channel: 3, name: "Keys" },
  { group: 1, channel: 0, name: "Drums" },
  { group: 1, channel: 1, name: "Tracks" },
];

const logs: AppLogEntry[] = [
  { id: 1, timestamp: "20:14:02", severity: "info", message: "Connected to LV1 at 192.168.1.42:22000" },
  { id: 2, timestamp: "20:14:18", severity: "info", message: "Stored fader targets for Scene 004" },
  { id: 3, timestamp: "20:15:01", severity: "warning", message: "Recall skipped because lockout is enabled" },
];

export const storedVerseScene: SceneConfig = {
  sceneId: "scene-verse",
  sceneIndex: 3,
  sceneName: "Verse",
  durationMs: 2500,
  scopeToggles: { faders: true, pan: true },
  scopedChannels: [
    { group: 0, channel: 0 },
    { group: 0, channel: 2 },
    { group: 1, channel: 0 },
  ],
  channelConfigs: [
    { group: 0, channel: 0, faderDb: -3.5, pan: 0, balance: null, width: null, panMode: "mono" },
    { group: 0, channel: 1, faderDb: -8, pan: -0.2, balance: null, width: null, panMode: "mono" },
    { group: 0, channel: 2, faderDb: -6, pan: null, balance: 0.15, width: 0.6, panMode: "stereo" },
    { group: 1, channel: 0, faderDb: -5, pan: null, balance: null, width: null, panMode: "none" },
  ],
};

export const storedChorusScene: SceneConfig = {
  ...storedVerseScene,
  sceneId: "scene-chorus",
  sceneIndex: 4,
  sceneName: "Chorus",
  durationMs: 4000,
  scopeToggles: { faders: true, pan: false },
};

export const duplicateVerseScene: SceneConfig = {
  ...storedVerseScene,
  sceneId: "scene-verse-duplicate",
  sceneIndex: 9,
  sceneName: "Verse",
  durationMs: 1500,
};

export const connectedAppState: AppViewState = {
  ...disconnectedAppViewState,
  connection: "connected",
  connectedLv1Identity: { uuid: "lv1-demo", host: "FOH LV1", address: "192.168.1.42", port: 22000 },
  currentScene: { index: 3, name: "Verse" },
  scenes: [
    { index: 3, name: "Verse" },
    { index: 4, name: "Chorus" },
    { index: 9, name: "Verse" },
  ],
  sceneCount: 3,
  channelCount: channels.length,
  channels,
  fadeState: "idle",
  logs,
  lastEventAt: "20:15:01",
  sceneConfigs: [storedVerseScene, storedChorusScene],
  selectedSceneId: storedVerseScene.sceneId,
  showFileName: "Sunday Service.ascshow",
  showFilePath: "/Users/engineer/Shows/Sunday Service.ascshow",
  showFileDirty: true,
  showFileLastSavedAt: "20:12:40",
  stateVersion: 12,
};

export const connectedWithDuplicateScenesAppState: AppViewState = {
  ...connectedAppState,
  sceneConfigs: [storedVerseScene, storedChorusScene, duplicateVerseScene],
};

export const discoveringAppState: AppViewState = {
  ...disconnectedAppViewState,
  discoveredLv1Systems: [],
};

export const discoveredSystemsAppState: AppViewState = {
  ...disconnectedAppViewState,
  discoveredLv1Systems: [
    {
      identity: { uuid: "lv1-demo", host: "FOH LV1", address: "192.168.1.42", port: 22000 },
      latencyMs: 3,
      status: "available",
    },
    {
      identity: { uuid: null, host: null, address: "192.168.1.43", port: 22000 },
      latencyMs: null,
      status: "unavailable",
    },
  ],
};

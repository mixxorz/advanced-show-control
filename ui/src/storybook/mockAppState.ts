import { disconnectedAppViewState, type AppLogEntry, type AppViewState, type SceneConfig } from "../types";

type ChannelSummary = AppViewState["channels"][number];
type DiscoveredLv1System = AppViewState["discoveredLv1Systems"][number];
type SceneSummary = AppViewState["scenes"][number];

function makeChannels(): ChannelSummary[] {
  return [
    { group: 0, channel: 0, name: "Lead Vocal" },
    { group: 0, channel: 1, name: "B Vocal" },
    { group: 0, channel: 2, name: "Guitar" },
    { group: 0, channel: 3, name: "Keys" },
    { group: 1, channel: 0, name: "Drums" },
    { group: 1, channel: 1, name: "Tracks" },
  ];
}

function makeLogs(): AppLogEntry[] {
  return [
    { id: 1, timestamp: "20:14:02", severity: "info", message: "Connected to LV1 at 192.168.1.42:22000" },
    { id: 2, timestamp: "20:14:18", severity: "info", message: "Stored fader targets for Scene 004" },
    { id: 3, timestamp: "20:15:01", severity: "warning", message: "Recall skipped because lockout is enabled" },
  ];
}

function makeSceneSummaries(): SceneSummary[] {
  return [
    { index: 3, name: "Verse" },
    { index: 4, name: "Chorus" },
    { index: 9, name: "Verse" },
  ];
}

function makeConnectedIdentity() {
  return { uuid: "lv1-demo", host: "FOH LV1", address: "192.168.1.42", port: 22000 };
}

function makeStoredVerseScene(): SceneConfig {
  return {
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
}

function makeStoredChorusScene(): SceneConfig {
  return {
    sceneId: "scene-chorus",
    sceneIndex: 4,
    sceneName: "Chorus",
    durationMs: 4000,
    scopeToggles: { faders: true, pan: false },
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
}

function makeDuplicateVerseScene(): SceneConfig {
  return {
    sceneId: "scene-verse-duplicate",
    sceneIndex: 9,
    sceneName: "Verse",
    durationMs: 1500,
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
}

function makeDiscoveredSystems(): DiscoveredLv1System[] {
  return [
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
  ];
}

function makeBaseDisconnectedAppState(overrides: Partial<AppViewState> = {}): AppViewState {
  return {
    ...disconnectedAppViewState,
    discoveredLv1Systems: [],
    reconnect: { active: false, attempt: 0 },
    scenes: [],
    channels: [],
    logs: [],
    sceneConfigs: [],
    ...overrides,
  };
}

function makeConnectedAppState(sceneConfigs = [makeStoredVerseScene(), makeStoredChorusScene()]): AppViewState {
  const channels = makeChannels();
  const logs = makeLogs();
  const scenes = makeSceneSummaries();
  const connectedLv1Identity = makeConnectedIdentity();

  return makeBaseDisconnectedAppState({
    connection: "connected",
    connectedLv1Identity,
    currentScene: { index: 3, name: "Verse" },
    scenes,
    sceneCount: scenes.length,
    channelCount: channels.length,
    channels,
    fadeState: "idle",
    logs,
    lastEventAt: "20:15:01",
    sceneConfigs,
    selectedSceneId: sceneConfigs[0]?.sceneId ?? null,
    showFileName: "Sunday Service.ascshow",
    showFilePath: "/Users/engineer/Shows/Sunday Service.ascshow",
    showFileDirty: true,
    showFileLastSavedAt: "20:12:40",
    stateVersion: 12,
  });
}

export const storedVerseScene: SceneConfig = makeStoredVerseScene();

export const storedChorusScene: SceneConfig = makeStoredChorusScene();

export const duplicateVerseScene: SceneConfig = makeDuplicateVerseScene();

export const connectedAppState: AppViewState = makeConnectedAppState();

export const connectedWithDuplicateScenesAppState: AppViewState = makeConnectedAppState([
  makeStoredVerseScene(),
  makeStoredChorusScene(),
  makeDuplicateVerseScene(),
]);

export const discoveringAppState: AppViewState = makeBaseDisconnectedAppState();

export const discoveredSystemsAppState: AppViewState = makeBaseDisconnectedAppState({
  discoveredLv1Systems: makeDiscoveredSystems(),
});

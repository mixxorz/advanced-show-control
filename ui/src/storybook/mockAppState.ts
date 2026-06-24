import {
  disconnectedAppViewState,
  type AppLogEntry,
  type AppViewState,
  type ChannelConfig,
  type SceneConfig,
} from "../types";

type ChannelSummary = AppViewState["channels"][number];
type DiscoveredLv1System = AppViewState["discoveredLv1Systems"][number];
type SceneSummary = AppViewState["scenes"][number];

function makeChannels(): ChannelSummary[] {
  return [
    ...makeNumberedChannels(0, 80, "Ch"),
    ...makeNumberedChannels(1, 16, "Group"),
    ...makeNumberedChannels(2, 32, "Aux"),
    ...makeNumberedChannels(6, 8, "Matrix"),
    ...makeNumberedChannels(12, 16, "DCA"),
    { group: 3, channel: 0, name: "LR" },
    { group: 4, channel: 0, name: "C" },
    { group: 5, channel: 0, name: "M" },
  ];
}

function makeNumberedChannels(
  group: number,
  count: number,
  label: string,
): ChannelSummary[] {
  return Array.from({ length: count }, (_, channel) => ({
    group,
    channel,
    name: `${label} ${String(channel + 1).padStart(2, "0")}`,
  }));
}

function makeChannelConfigs(): ChannelConfig[] {
  return makeChannels().map((channel) => ({
    group: channel.group,
    channel: channel.channel,
    faderDb: -3 - (channel.channel % 8),
    pan: channel.group === 0 ? ((channel.channel % 9) - 4) / 10 : null,
    balance: channel.group === 1 ? ((channel.channel % 7) - 3) / 10 : null,
    width: channel.group === 1 ? 0.6 : null,
    panMode:
      channel.group === 0 ? "mono" : channel.group === 1 ? "stereo" : "none",
  }));
}

function makeLogs(): AppLogEntry[] {
  return [
    {
      id: 1,
      timestamp: "20:14:02",
      severity: "info",
      message: "Connected to LV1 at 192.168.1.42:22000",
    },
    {
      id: 2,
      timestamp: "20:14:18",
      severity: "info",
      message: "Stored fader targets for Scene 004",
    },
    {
      id: 3,
      timestamp: "20:15:01",
      severity: "warning",
      message: "Recall skipped because lockout is enabled",
    },
  ];
}

function makeSceneSummaries(): SceneSummary[] {
  return [
    { index: 0, name: "Service Start" },
    { index: 1, name: "Tuning: A" },
    { index: 2, name: "S01: The Wonderful Blood" },
  ];
}

function makeConnectedIdentity() {
  return {
    uuid: "lv1-demo",
    host: "FOH LV1",
    address: "192.168.1.42",
    port: 22000,
  };
}

function makeStoredVerseScene(): SceneConfig {
  return {
    internalSceneId: "scene-verse",
    sceneIndex: 3,
    sceneName: "S01: The Wonderful Blood",
    durationMs: 2500,
    scopeToggles: { faders: true, pan: false },
    scopedChannels: [
      { group: 0, channel: 0 },
      { group: 0, channel: 2 },
      { group: 1, channel: 0 },
      { group: 2, channel: 0 },
      { group: 6, channel: 0 },
      { group: 12, channel: 0 },
      { group: 3, channel: 0 },
    ],
    channelConfigs: makeChannelConfigs(),
  };
}

function makeStoredChorusScene(): SceneConfig {
  return {
    internalSceneId: "scene-chorus",
    sceneIndex: 4,
    sceneName: "S02: Holy Forever",
    durationMs: 4000,
    scopeToggles: { faders: true, pan: false },
    scopedChannels: [
      { group: 0, channel: 0 },
      { group: 0, channel: 2 },
      { group: 1, channel: 0 },
      { group: 2, channel: 1 },
      { group: 6, channel: 1 },
      { group: 12, channel: 1 },
      { group: 5, channel: 0 },
    ],
    channelConfigs: makeChannelConfigs(),
  };
}

function makeDuplicateVerseScene(): SceneConfig {
  return {
    internalSceneId: "scene-verse-duplicate",
    sceneIndex: 9,
    sceneName: "S01: The Wonderful Blood",
    durationMs: 1500,
    scopeToggles: { faders: true, pan: true },
    scopedChannels: [
      { group: 0, channel: 0 },
      { group: 0, channel: 2 },
      { group: 1, channel: 0 },
      { group: 2, channel: 2 },
      { group: 6, channel: 2 },
      { group: 12, channel: 2 },
      { group: 4, channel: 0 },
    ],
    channelConfigs: makeChannelConfigs(),
  };
}

function makeUnlinkedDraftScene(): SceneConfig {
  return {
    internalSceneId: "scene-draft-unlinked",
    sceneIndex: null,
    sceneName: "Deleted Draft Scene",
    durationMs: 2000,
    scopeToggles: { faders: true, pan: false },
    scopedChannels: [
      { group: 0, channel: 0 },
      { group: 0, channel: 2 },
    ],
    channelConfigs: makeChannelConfigs(),
  };
}

function makeDiscoveredSystems(): DiscoveredLv1System[] {
  return [
    {
      identity: {
        uuid: "lv1-demo",
        host: "FOH LV1",
        address: "192.168.1.42",
        port: 22000,
      },
      latencyMs: 3,
      status: "available",
    },
    {
      identity: {
        uuid: null,
        host: null,
        address: "192.168.1.43",
        port: 22000,
      },
      latencyMs: null,
      status: "unavailable",
    },
  ];
}

function makeBaseDisconnectedAppState(
  overrides: Partial<AppViewState> = {},
): AppViewState {
  return {
    settings: disconnectedAppViewState.settings,
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
    cuedSceneInternalId: null,
    selectedSceneInternalId: null,
    showFileName: "Untitled Session",
    showFilePath: null,
    showFileDirty: false,
    showFileLastSavedAt: null,
    stateVersion: 0,
    ...overrides,
  };
}

function makeConnectedAppState(
  sceneConfigs = [makeStoredVerseScene(), makeStoredChorusScene()],
): AppViewState {
  const channels = makeChannels();
  const logs = makeLogs();
  const scenes = makeSceneSummaries();
  const connectedLv1Identity = makeConnectedIdentity();

  return makeBaseDisconnectedAppState({
    connection: "connected",
    connectedLv1Identity,
    currentScene: { index: 2, name: "S01: The Wonderful Blood" },
    scenes,
    sceneCount: scenes.length,
    channelCount: channels.length,
    channels,
    fadeState: "idle",
    logs,
    lastEventAt: "20:15:01",
    sceneConfigs,
    selectedSceneInternalId: sceneConfigs[0]?.internalSceneId ?? null,
    showFileName: "Sunday Service.ascs",
    showFilePath: "/Users/engineer/Sessions/Sunday Service.ascs",
    showFileDirty: true,
    showFileLastSavedAt: "20:12:40",
    stateVersion: 12,
  });
}

export const storedVerseScene: SceneConfig = makeStoredVerseScene();

export const storedChorusScene: SceneConfig = makeStoredChorusScene();

export const duplicateVerseScene: SceneConfig = makeDuplicateVerseScene();

export const unlinkedDraftScene: SceneConfig = makeUnlinkedDraftScene();

export const connectedAppState: AppViewState = makeConnectedAppState();

export const mockAppState: AppViewState = connectedAppState;

export const connectedWithDuplicateScenesAppState: AppViewState =
  makeConnectedAppState([
    makeStoredVerseScene(),
    makeStoredChorusScene(),
    makeDuplicateVerseScene(),
  ]);

export const connectedWithUnlinkedSceneAppState: AppViewState =
  makeConnectedAppState([
    makeStoredVerseScene(),
    makeUnlinkedDraftScene(),
    makeStoredChorusScene(),
  ]);

export const discoveringAppState: AppViewState = makeBaseDisconnectedAppState();

export const discoveredSystemsAppState: AppViewState =
  makeBaseDisconnectedAppState({
    discoveredLv1Systems: makeDiscoveredSystems(),
  });

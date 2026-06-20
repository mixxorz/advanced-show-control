import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { AppRuntime, type AppRuntimeServices } from "./AppRuntime";
import type { AppViewState } from "./types";
import {
  attemptReconnectLv1,
  connectLv1System,
  reconnectTimedOut,
  refreshLv1Discovery,
  startupAutoConnectLv1,
} from "./commands";

const services: AppRuntimeServices = {
  frontendReady: () => invoke<void>("frontend_ready"),
  abortAll: () => invoke<void>("abort_all_fades"),
  attemptReconnectLv1,
  connectLv1System,
  disconnectLv1: () => invoke<void>("disconnect_lv1"),
  listenForAppStatus: (listener) =>
    listen<AppViewState>("app-status-changed", (event) =>
      listener(event.payload),
    ),
  newShowFile: () => invoke<void>("new_show_file"),
  openShowFile: () => invoke<void>("open_show_file_dialog"),
  cueScene: (sceneId) => invoke<void>("cue_scene", { sceneId }),
  recallScene: (sceneId) => invoke<void>("recall_scene", { sceneId }),
  reconnectTimedOut,
  refreshLv1Discovery,
  saveShowFile: () => invoke<void>("save_show_file"),
  saveShowFileAs: () => invoke<void>("save_show_file_as_dialog"),
  selectSceneConfig: (sceneId) =>
    invoke<void>("select_scene_config", { sceneId }),
  setAllChannelsScoped: (sceneId, scoped) =>
    invoke<void>("set_all_channels_scoped", { sceneId, scoped }),
  setChannelScoped: (sceneId, group, channel, scoped) =>
    invoke<void>("set_channel_scoped", {
      sceneId,
      group,
      channel,
      scoped,
    }),
  setLockout: (enabled) => invoke<void>("set_lockout", { enabled }),
  setSceneDurationMs: (sceneId, durationMs) =>
    invoke<void>("set_scene_duration_ms", { sceneId, durationMs }),
  setSceneScopeFadersEnabled: (sceneId, enabled) =>
    invoke<void>("set_scene_scope_faders_enabled", {
      sceneId,
      enabled,
    }),
  setSceneScopePanEnabled: (sceneId, enabled) =>
    invoke<void>("set_scene_scope_pan_enabled", { sceneId, enabled }),
  startupAutoConnectLv1,
  storeSceneConfig: (sceneId) =>
    invoke<void>("store_scene_config", { sceneId }),
};

export default function App() {
  return <AppRuntime services={services} />;
}

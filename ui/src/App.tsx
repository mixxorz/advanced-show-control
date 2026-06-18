import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { AppRuntime, type AppRuntimeServices } from "./AppRuntime";
import {
  attemptReconnectLv1,
  connectLv1System,
  reconnectTimedOut,
  refreshLv1Discovery,
  startupAutoConnectLv1,
} from "./commands";
import type { AppViewState } from "./types";

const services: AppRuntimeServices = {
  abortAll: () => invoke("abort_all_fades"),
  attemptReconnectLv1,
  connectLv1System,
  disconnectLv1: () => invoke<AppViewState>("disconnect_lv1"),
  listenForAppStatus: (listener) =>
    listen<AppViewState>("app-status-changed", (event) =>
      listener(event.payload),
    ),
  newShowFile: () => invoke<AppViewState>("new_show_file"),
  openShowFile: () => invoke<AppViewState>("open_show_file_dialog"),
  reconnectTimedOut,
  refreshAppState: () => invoke<AppViewState>("get_app_status"),
  refreshLv1Discovery,
  saveShowFile: () => invoke<AppViewState>("save_show_file"),
  saveShowFileAs: () => invoke<AppViewState>("save_show_file_as_dialog"),
  selectSceneConfig: (sceneId) =>
    invoke<AppViewState>("select_scene_config", { sceneId }),
  setAllChannelsScoped: (sceneId, scoped) =>
    invoke<AppViewState>("set_all_channels_scoped", { sceneId, scoped }),
  setChannelScoped: (sceneId, group, channel, scoped) =>
    invoke<AppViewState>("set_channel_scoped", {
      sceneId,
      group,
      channel,
      scoped,
    }),
  setLockout: (enabled) => invoke<AppViewState>("set_lockout", { enabled }),
  setSceneDurationMs: (sceneId, durationMs) =>
    invoke<AppViewState>("set_scene_duration_ms", { sceneId, durationMs }),
  setSceneScopeFadersEnabled: (sceneId, enabled) =>
    invoke<AppViewState>("set_scene_scope_faders_enabled", {
      sceneId,
      enabled,
    }),
  setSceneScopePanEnabled: (sceneId, enabled) =>
    invoke<AppViewState>("set_scene_scope_pan_enabled", { sceneId, enabled }),
  startupAutoConnectLv1,
  storeSceneConfig: (sceneId) =>
    invoke<AppViewState>("store_scene_config", { sceneId }),
};

export default function App() {
  return <AppRuntime services={services} />;
}

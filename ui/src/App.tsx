import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AppRuntime, type AppRuntimeServices } from "./AppRuntime";
import type { AppViewState } from "./types";
import {
  addSceneToActiveCueList,
  attemptReconnectLv1,
  createCueList,
  cueEntry,
  recallCuedCue,
  connectLv1System,
  deleteCueList,
  deleteSceneConfig,
  linkSceneConfig,
  probeLv1TcpConnectLatency,
  reconnectTimedOut,
  refreshLv1Discovery,
  removeCueEntry,
  renameCueList,
  reorderCueEntries,
  reorderCueLists,
  setActiveCueList,
  startupAutoConnectLv1,
} from "./commands";

const services: AppRuntimeServices = {
  frontendReady: () => invoke<void>("frontend_ready"),
  abortAll: () => invoke<void>("abort_all_fades"),
  addSceneToActiveCueList,
  attemptReconnectLv1,
  connectLv1System,
  createCueList,
  cueEntry,
  deleteCueList,
  disconnectLv1: () => invoke<void>("disconnect_lv1"),
  listenForAppStatus: (listener) =>
    listen<AppViewState>("app-status-changed", (event) =>
      listener(event.payload),
    ),
  newShowFile: () => invoke<void>("new_show_file"),
  openShowFile: () => invoke<void>("open_show_file_dialog"),
  removeCueEntry,
  recallCuedCue,
  recallScene: (internalSceneId) =>
    invoke<void>("recall_scene", { internalSceneId }),
  renameCueList,
  probeLv1TcpConnectLatency,
  reconnectTimedOut,
  reorderCueEntries,
  reorderCueLists,
  refreshLv1Discovery,
  saveShowFile: () => invoke<void>("save_show_file"),
  saveShowFileAs: () => invoke<void>("save_show_file_as_dialog"),
  selectSceneConfig: (internalSceneId) =>
    invoke<void>("select_scene_config", { internalSceneId }),
  setAllChannelsScoped: (internalSceneId, scoped) =>
    invoke<void>("set_all_channels_scoped", { internalSceneId, scoped }),
  setActiveCueList,
  setChannelScoped: (internalSceneId, group, channel, scoped) =>
    invoke<void>("set_channel_scoped", {
      internalSceneId,
      group,
      channel,
      scoped,
    }),
  setLockout: (enabled) => invoke<void>("set_lockout", { enabled }),
  setSceneDurationMs: (internalSceneId, durationMs) =>
    invoke<void>("set_scene_duration_ms", { internalSceneId, durationMs }),
  setSceneScopeFadersEnabled: (internalSceneId, enabled) =>
    invoke<void>("set_scene_scope_faders_enabled", {
      internalSceneId,
      enabled,
    }),
  setSceneScopePanEnabled: (internalSceneId, enabled) =>
    invoke<void>("set_scene_scope_pan_enabled", { internalSceneId, enabled }),
  setWindowTitle: (title) => getCurrentWindow().setTitle(title),
  startupAutoConnectLv1,
  storeSceneConfig: (internalSceneId) =>
    invoke<void>("store_scene_config", { internalSceneId }),
  linkSceneConfig,
  deleteSceneConfig,
};

export default function App() {
  return <AppRuntime services={services} />;
}

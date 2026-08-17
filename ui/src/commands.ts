import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  Lv1SystemIdentity,
  TcpConnectLatencyResult,
} from "./types";

export function frontendReady(): Promise<void> {
  return invoke<void>("frontend_ready");
}

export function startupAutoConnectLv1(): Promise<void> {
  return invoke<void>("startup_auto_connect_lv1");
}

export function refreshLv1Discovery(): Promise<void> {
  return invoke<void>("refresh_lv1_discovery", { timeoutMs: 1000 });
}

export function connectLv1System(identity: Lv1SystemIdentity): Promise<void> {
  return invoke<void>("connect_lv1_system", { identity });
}

export function disconnectLv1(): Promise<void> {
  return invoke<void>("disconnect_lv1");
}

export function probeLv1TcpConnectLatency(
  identity: Lv1SystemIdentity,
  timeoutMs?: number,
): Promise<TcpConnectLatencyResult> {
  return invoke<TcpConnectLatencyResult>("probe_lv1_tcp_connect_latency", {
    identity,
    ...(timeoutMs !== undefined ? { timeoutMs } : {}),
  });
}

export function abortAll(): Promise<void> {
  return invoke<void>("abort_all_fades");
}

export function newShowFile(): Promise<void> {
  return invoke<void>("new_show_file");
}

export function openShowFile(): Promise<void> {
  return invoke<void>("open_show_file_dialog");
}

export function saveShowFile(): Promise<void> {
  return invoke<void>("save_show_file");
}

export function saveShowFileAs(): Promise<void> {
  return invoke<void>("save_show_file_as_dialog");
}

export function recallScene(internalSceneId: string): Promise<void> {
  return invoke<void>("recall_scene", { internalSceneId });
}

export function selectSceneConfig(internalSceneId: string): Promise<void> {
  return invoke<void>("select_scene_config", { internalSceneId });
}

export function storeSceneConfig(internalSceneId: string): Promise<void> {
  return invoke<void>("store_scene_config", { internalSceneId });
}

export function setAllChannelsScoped(
  internalSceneId: string,
  scoped: boolean,
): Promise<void> {
  return invoke<void>("set_all_channels_scoped", {
    internalSceneId,
    scoped,
  });
}

export function setChannelScoped(
  internalSceneId: string,
  group: number,
  channel: number,
  scoped: boolean,
): Promise<void> {
  return invoke<void>("set_channel_scoped", {
    internalSceneId,
    group,
    channel,
    scoped,
  });
}

export function setLockout(enabled: boolean): Promise<void> {
  return invoke<void>("set_lockout", { enabled });
}

export function setSceneDurationMs(
  internalSceneId: string,
  durationMs: number,
): Promise<void> {
  return invoke<void>("set_scene_duration_ms", {
    internalSceneId,
    durationMs,
  });
}

export function setSceneScopeFadersEnabled(
  internalSceneId: string,
  enabled: boolean,
): Promise<void> {
  return invoke<void>("set_scene_scope_faders_enabled", {
    internalSceneId,
    enabled,
  });
}

export function setSceneScopePanEnabled(
  internalSceneId: string,
  enabled: boolean,
): Promise<void> {
  return invoke<void>("set_scene_scope_pan_enabled", {
    internalSceneId,
    enabled,
  });
}

export function linkSceneConfig(
  sourceInternalSceneId: string,
  targetSceneIndex: number,
  overwriteExisting: boolean,
): Promise<void> {
  return invoke<void>("link_scene_config", {
    sourceInternalSceneId,
    targetSceneIndex,
    overwriteExisting,
  });
}

export function deleteSceneConfig(internalSceneId: string): Promise<void> {
  return invoke<void>("delete_scene_config", { internalSceneId });
}

export function copySceneSettings(internalSceneId: string): Promise<void> {
  return invoke<void>("copy_scene_settings", { internalSceneId });
}

export function pasteSceneSettings(internalSceneId: string): Promise<void> {
  return invoke<void>("paste_scene_settings", { internalSceneId });
}

export function createCueList(name: string): Promise<void> {
  return invoke<void>("create_cue_list", { name });
}

export function renameCueList(cueListId: string, name: string): Promise<void> {
  return invoke<void>("rename_cue_list", { cueListId, name });
}

export function deleteCueList(cueListId: string): Promise<void> {
  return invoke<void>("delete_cue_list", { cueListId });
}

export function reorderCueLists(orderedIds: string[]): Promise<void> {
  return invoke<void>("reorder_cue_lists", { orderedIds });
}

export function setActiveCueList(cueListId: string | null): Promise<void> {
  return invoke<void>("set_active_cue_list", { cueListId });
}

export function addSceneToActiveCueList(
  sceneInternalId: string,
  insertIndex: number,
): Promise<void> {
  return invoke<void>("add_scene_to_active_cue_list", {
    sceneInternalId,
    insertIndex,
  });
}

export function removeCueEntry(cueEntryId: string): Promise<void> {
  return invoke<void>("remove_cue_entry", { cueEntryId });
}

export function reorderCueEntries(orderedEntryIds: string[]): Promise<void> {
  return invoke<void>("reorder_cue_entries", { orderedEntryIds });
}

export function cueEntry(cueEntryId: string | null): Promise<void> {
  return invoke<void>("cue_entry", { cueEntryId });
}

export function recallCuedCue(): Promise<void> {
  return invoke<void>("recall_cued_cue");
}

export function replaceAppSettings(settings: AppSettings): Promise<void> {
  return invoke<void>("replace_app_settings", { settings });
}

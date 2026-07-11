import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  Lv1SystemIdentity,
  TcpConnectLatencyResult,
} from "./types";

export async function startupAutoConnectLv1() {
  return invoke<void>("startup_auto_connect_lv1");
}

export async function refreshLv1Discovery() {
  return invoke<void>("refresh_lv1_discovery", { timeoutMs: 1000 });
}

export async function connectLv1System(identity: Lv1SystemIdentity) {
  return invoke<void>("connect_lv1_system", { identity });
}

export async function reconnectTimedOut(attempt: number) {
  return invoke<void>("reconnect_timed_out", { attempt });
}

export async function attemptReconnectLv1() {
  return invoke<void>("attempt_reconnect_lv1");
}

export async function probeLv1TcpConnectLatency(
  identity: Lv1SystemIdentity,
  timeoutMs?: number,
) {
  return invoke<TcpConnectLatencyResult>("probe_lv1_tcp_connect_latency", {
    identity,
    ...(timeoutMs !== undefined ? { timeoutMs } : {}),
  });
}

export async function setSceneScopePanEnabled(
  internalSceneId: string,
  enabled: boolean,
) {
  return invoke<void>("set_scene_scope_pan_enabled", {
    internalSceneId,
    enabled,
  });
}

export async function linkSceneConfig(
  sourceInternalSceneId: string,
  targetSceneIndex: number,
  overwriteExisting: boolean,
) {
  return invoke<void>("link_scene_config", {
    sourceInternalSceneId,
    targetSceneIndex,
    overwriteExisting,
  });
}

export async function deleteSceneConfig(internalSceneId: string) {
  return invoke<void>("delete_scene_config", { internalSceneId });
}

export async function copySceneSettings(internalSceneId: string) {
  return invoke<void>("copy_scene_settings", { internalSceneId });
}

export async function pasteSceneSettings(internalSceneId: string) {
  return invoke<void>("paste_scene_settings", { internalSceneId });
}

export async function createCueList(name: string) {
  return invoke<void>("create_cue_list", { name });
}

export async function renameCueList(cueListId: string, name: string) {
  return invoke<void>("rename_cue_list", { cueListId, name });
}

export async function deleteCueList(cueListId: string) {
  return invoke<void>("delete_cue_list", { cueListId });
}

export async function reorderCueLists(orderedIds: string[]) {
  return invoke<void>("reorder_cue_lists", { orderedIds });
}

export async function setActiveCueList(cueListId: string | null) {
  return invoke<void>("set_active_cue_list", { cueListId });
}

export async function addSceneToActiveCueList(
  sceneInternalId: string,
  insertIndex: number,
) {
  return invoke<void>("add_scene_to_active_cue_list", {
    sceneInternalId,
    insertIndex,
  });
}

export async function removeCueEntry(cueEntryId: string) {
  return invoke<void>("remove_cue_entry", { cueEntryId });
}

export async function reorderCueEntries(orderedEntryIds: string[]) {
  return invoke<void>("reorder_cue_entries", { orderedEntryIds });
}

export async function cueEntry(cueEntryId: string | null) {
  return invoke<void>("cue_entry", { cueEntryId });
}

export async function recallCuedCue() {
  return invoke<void>("recall_cued_cue");
}

export async function replaceAppSettings(settings: AppSettings) {
  return invoke<void>("replace_app_settings", { settings });
}

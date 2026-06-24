import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, Lv1SystemIdentity } from "./types";

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

export async function replaceAppSettings(settings: AppSettings) {
  return invoke<void>("replace_app_settings", { settings });
}

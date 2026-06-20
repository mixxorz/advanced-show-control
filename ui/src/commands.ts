import { invoke } from "@tauri-apps/api/core";
import type { Lv1SystemIdentity } from "./types";

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
  sceneId: string,
  enabled: boolean,
) {
  return invoke<void>("set_scene_scope_pan_enabled", { sceneId, enabled });
}

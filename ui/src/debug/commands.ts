import { invoke } from "@tauri-apps/api/core";
import type { Lv1SystemIdentity } from "../types";

export type SmokeBackendResult = {
  ok: boolean;
  testId: string;
  startedAt: string;
  finishedAt: string;
  steps: Array<{
    ok: boolean;
    step: string;
    message: string;
    observed?: unknown;
  }>;
  observedEvents: string[];
  observedTraces: unknown[];
};

export type SmokeTestParams = {
  sceneAId: string;
  sceneBId: string;
  channel: { group: number; channel: number };
  toleranceDb: number;
  minimumMovementDb: number;
  timeoutMs: number;
  sampleIntervalMs: number;
};

export async function runConnectionTest(identity: Lv1SystemIdentity) {
  return invoke<SmokeBackendResult>("debug_smoke_run_connection_test", {
    identity,
    timeoutMs: 10000,
  });
}

export async function runSceneRecallTest(params: SmokeTestParams) {
  return invoke<SmokeBackendResult>("debug_smoke_run_scene_recall_test", {
    params,
    targetSceneId: params.sceneAId,
  });
}

export async function runFadeStartsTest(params: SmokeTestParams) {
  return invoke<SmokeBackendResult>("debug_smoke_run_fade_starts_test", {
    params,
  });
}

export async function runFadeCompletesTest(
  params: SmokeTestParams,
  expectedTargetDb = 0,
) {
  return invoke<SmokeBackendResult>("debug_smoke_run_fade_completes_test", {
    params,
    expectedTargetDb,
  });
}

export async function runDecreasingXFadeTest(params: SmokeTestParams) {
  return invoke<SmokeBackendResult>("debug_smoke_run_decreasing_xfade_test", {
    params,
  });
}

export async function runLockoutBlocksRecallTest(params: SmokeTestParams) {
  return invoke<SmokeBackendResult>(
    "debug_smoke_run_lockout_blocks_recall_test",
    {
      params,
    },
  );
}

export async function finishSmokeSuite(
  ok: boolean,
  message?: string,
): Promise<void> {
  await invoke("debug_smoke_finish_suite", { ok, message });
}

export async function exitSmokeApp(): Promise<void> {
  await invoke("debug_smoke_exit_app");
}

export async function reportSmokeSetup(params: SmokeTestParams): Promise<void> {
  await invoke("debug_smoke_report_setup", { params });
}

export async function setSmokeChannelGain(
  params: SmokeTestParams,
  gainDb: number,
): Promise<void> {
  await invoke("debug_smoke_set_channel_gain", { params, gainDb });
}

export async function refreshLv1Discovery(): Promise<void> {
  await invoke("refresh_lv1_discovery");
}

export async function newShowFile(): Promise<void> {
  await invoke("new_show_file");
}

export async function recallScene(sceneId: string): Promise<void> {
  await invoke("recall_scene", { sceneId });
}

export async function storeSceneConfig(sceneId: string): Promise<void> {
  await invoke("store_scene_config", { sceneId });
}

export async function setChannelScoped(
  sceneId: string,
  channel: SmokeTestParams["channel"],
  scoped: boolean,
): Promise<void> {
  await invoke("set_channel_scoped", {
    sceneId,
    group: channel.group,
    channel: channel.channel,
    scoped,
  });
}

export async function setSceneDurationMs(
  sceneId: string,
  durationMs: number,
): Promise<void> {
  await invoke("set_scene_duration_ms", { sceneId, durationMs });
}

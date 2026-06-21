import { invoke } from "@tauri-apps/api/core";
import type { Lv1SystemIdentity } from "../types";

export type SmokeBackendResult = {
  ok: boolean;
  message: string;
  steps: Array<{
    ok: boolean;
    step: string;
    message: string;
    observed?: unknown;
  }>;
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

export async function runFadeCompletesTest(params: SmokeTestParams) {
  return invoke<SmokeBackendResult>("debug_smoke_run_fade_completes_test", {
    params,
    expectedTargetDb: params.toleranceDb,
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

export async function refreshLv1Discovery(): Promise<void> {
  await invoke("refresh_lv1_discovery");
}

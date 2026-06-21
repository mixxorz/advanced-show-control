import { invoke } from "@tauri-apps/api/core";
import type { Lv1SystemIdentity } from "../types";
import type { SmokeBackendResult } from "./smokeTypes";

export function runConnectionTest(
  identity: Lv1SystemIdentity,
  timeoutMs: number,
) {
  return invoke<SmokeBackendResult>("debug_smoke_run_connection_test", {
    identity,
    timeoutMs,
  });
}

export function runSceneRecallTest(sceneId: string, timeoutMs: number) {
  return invoke<SmokeBackendResult>("debug_smoke_run_scene_recall_test", {
    sceneId,
    timeoutMs,
  });
}

export function runFadeStartsTest(sceneId: string, timeoutMs: number) {
  return invoke<SmokeBackendResult>("debug_smoke_run_fade_starts_test", {
    sceneId,
    timeoutMs,
  });
}

export function runFadeCompletesTest(
  sceneId: string,
  toleranceDb: number,
  minimumMovementDb: number,
  timeoutMs: number,
  sampleIntervalMs: number,
) {
  return invoke<SmokeBackendResult>("debug_smoke_run_fade_completes_test", {
    sceneId,
    toleranceDb,
    minimumMovementDb,
    timeoutMs,
    sampleIntervalMs,
  });
}

export function runDecreasingXfadeTest(params: {
  sceneAId: string;
  sceneBId: string;
  timeoutMs: number;
  toleranceDb: number;
  minimumMovementDb: number;
}) {
  return invoke<SmokeBackendResult>("debug_smoke_run_decreasing_xfade_test", {
    params,
  });
}

export function runLockoutBlocksRecallTest(
  sceneId: string,
  blockedSceneId: string,
  timeoutMs: number,
) {
  return invoke<SmokeBackendResult>(
    "debug_smoke_run_lockout_blocks_recall_test",
    {
      sceneId,
      blockedSceneId,
      timeoutMs,
    },
  );
}

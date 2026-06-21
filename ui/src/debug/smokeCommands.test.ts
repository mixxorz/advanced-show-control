import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Lv1SystemIdentity } from "../types";
import {
  runConnectionTest,
  runDecreasingXfadeTest,
  runFadeCompletesTest,
  runFadeStartsTest,
  runLockoutBlocksRecallTest,
  runSceneRecallTest,
} from "./smokeCommands";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("smokeCommands", () => {
  const mockedInvoke = vi.mocked(invoke);

  beforeEach(() => {
    mockedInvoke.mockReset();
    mockedInvoke.mockResolvedValue({ ok: true, message: "ok", steps: [] });
  });

  it("invokes decreasing xfade command", async () => {
    const params = {
      sceneAId: "scene-a",
      sceneBId: "scene-b",
      timeoutMs: 1000,
      toleranceDb: 0.5,
      minimumMovementDb: 3,
    };

    await runDecreasingXfadeTest(params);

    expect(mockedInvoke).toHaveBeenCalledWith(
      "debug_smoke_run_decreasing_xfade_test",
      { params },
    );
  });

  it("invokes the remaining smoke commands", async () => {
    const identity: Lv1SystemIdentity = {
      uuid: null,
      host: null,
      address: "192.168.1.10",
      port: 12345,
    };

    await runConnectionTest(identity, 1200);
    await runSceneRecallTest("scene-1", 1300);
    await runFadeStartsTest("scene-1", 1400);
    await runFadeCompletesTest("scene-1", 0.5, 3, 1500, 250);
    await runLockoutBlocksRecallTest("scene-a", "scene-b", 1600);

    expect(mockedInvoke).toHaveBeenCalledWith(
      "debug_smoke_run_connection_test",
      { identity, timeoutMs: 1200 },
    );
    expect(mockedInvoke).toHaveBeenCalledWith(
      "debug_smoke_run_scene_recall_test",
      { sceneId: "scene-1", timeoutMs: 1300 },
    );
    expect(mockedInvoke).toHaveBeenCalledWith(
      "debug_smoke_run_fade_starts_test",
      { sceneId: "scene-1", timeoutMs: 1400 },
    );
    expect(mockedInvoke).toHaveBeenCalledWith(
      "debug_smoke_run_fade_completes_test",
      {
        sceneId: "scene-1",
        toleranceDb: 0.5,
        minimumMovementDb: 3,
        timeoutMs: 1500,
        sampleIntervalMs: 250,
      },
    );
    expect(mockedInvoke).toHaveBeenCalledWith(
      "debug_smoke_run_lockout_blocks_recall_test",
      {
        sceneId: "scene-a",
        blockedSceneId: "scene-b",
        timeoutMs: 1600,
      },
    );
  });
});

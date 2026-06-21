import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  runConnectionTest,
  runDecreasingXFadeTest,
  runFadeCompletesTest,
  runFadeStartsTest,
  runLockoutBlocksRecallTest,
  refreshLv1Discovery,
  runSceneRecallTest,
} from "./commands";

const invoke = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke,
}));

describe("smokeCommands", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("invokes the connection smoke command", async () => {
    invoke.mockResolvedValueOnce({});

    await runConnectionTest({
      uuid: null,
      host: null,
      address: "127.0.0.1",
      port: 12345,
    });

    expect(invoke).toHaveBeenCalledWith("debug_smoke_run_connection_test", {
      identity: { uuid: null, host: null, address: "127.0.0.1", port: 12345 },
      timeoutMs: 10000,
    });
  });

  it("invokes the scene recall smoke command", async () => {
    invoke.mockResolvedValueOnce({});

    await runSceneRecallTest({
      sceneAId: "scene-a",
      sceneBId: "scene-b",
      channel: { group: 1, channel: 2 },
      toleranceDb: 0.5,
      minimumMovementDb: 3,
      timeoutMs: 15000,
      sampleIntervalMs: 250,
    });

    expect(invoke).toHaveBeenCalledWith("debug_smoke_run_scene_recall_test", {
      params: {
        sceneAId: "scene-a",
        sceneBId: "scene-b",
        channel: { group: 1, channel: 2 },
        toleranceDb: 0.5,
        minimumMovementDb: 3,
        timeoutMs: 15000,
        sampleIntervalMs: 250,
      },
      targetSceneId: "scene-a",
    });
  });

  it("invokes the fade starts smoke command", async () => {
    invoke.mockResolvedValueOnce({});

    await runFadeStartsTest({
      sceneAId: "scene-a",
      sceneBId: "scene-b",
      channel: { group: 1, channel: 2 },
      toleranceDb: 0.5,
      minimumMovementDb: 3,
      timeoutMs: 15000,
      sampleIntervalMs: 250,
    });

    expect(invoke).toHaveBeenCalledWith("debug_smoke_run_fade_starts_test", {
      params: {
        sceneAId: "scene-a",
        sceneBId: "scene-b",
        channel: { group: 1, channel: 2 },
        toleranceDb: 0.5,
        minimumMovementDb: 3,
        timeoutMs: 15000,
        sampleIntervalMs: 250,
      },
    });
  });

  it("invokes the fade completes smoke command", async () => {
    invoke.mockResolvedValueOnce({});

    await runFadeCompletesTest({
      sceneAId: "scene-a",
      sceneBId: "scene-b",
      channel: { group: 1, channel: 2 },
      toleranceDb: -6.5,
      minimumMovementDb: 3,
      timeoutMs: 15000,
      sampleIntervalMs: 250,
    });

    expect(invoke).toHaveBeenCalledWith("debug_smoke_run_fade_completes_test", {
      params: {
        sceneAId: "scene-a",
        sceneBId: "scene-b",
        channel: { group: 1, channel: 2 },
        toleranceDb: -6.5,
        minimumMovementDb: 3,
        timeoutMs: 15000,
        sampleIntervalMs: 250,
      },
      expectedTargetDb: -6.5,
    });
  });

  it("invokes the decreasing xfade smoke command", async () => {
    invoke.mockResolvedValueOnce({});

    await runDecreasingXFadeTest({
      sceneAId: "scene-a",
      sceneBId: "scene-b",
      channel: { group: 1, channel: 2 },
      toleranceDb: 0.5,
      minimumMovementDb: 3,
      timeoutMs: 15000,
      sampleIntervalMs: 250,
    });

    expect(invoke).toHaveBeenCalledWith(
      "debug_smoke_run_decreasing_xfade_test",
      {
        params: {
          sceneAId: "scene-a",
          sceneBId: "scene-b",
          channel: { group: 1, channel: 2 },
          toleranceDb: 0.5,
          minimumMovementDb: 3,
          timeoutMs: 15000,
          sampleIntervalMs: 250,
        },
      },
    );
  });

  it("invokes the lockout smoke command", async () => {
    invoke.mockResolvedValueOnce({});

    await runLockoutBlocksRecallTest({
      sceneAId: "scene-a",
      sceneBId: "scene-b",
      channel: { group: 1, channel: 2 },
      toleranceDb: 0.5,
      minimumMovementDb: 3,
      timeoutMs: 15000,
      sampleIntervalMs: 250,
    });

    expect(invoke).toHaveBeenCalledWith(
      "debug_smoke_run_lockout_blocks_recall_test",
      {
        params: {
          sceneAId: "scene-a",
          sceneBId: "scene-b",
          channel: { group: 1, channel: 2 },
          toleranceDb: 0.5,
          minimumMovementDb: 3,
          timeoutMs: 15000,
          sampleIntervalMs: 250,
        },
      },
    );
  });

  it("invokes LV1 discovery refresh", async () => {
    invoke.mockResolvedValueOnce(undefined);

    await refreshLv1Discovery();

    expect(invoke).toHaveBeenCalledWith("refresh_lv1_discovery");
  });
});

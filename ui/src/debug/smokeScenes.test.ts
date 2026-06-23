import { describe, expect, test } from "vitest";
import type { SceneConfig } from "../types";
import { findSmokeSceneConfigs } from "./smokeScenes";

function scene(
  internalSceneId: string,
  sceneIndex: number | null,
  sceneName: string,
): SceneConfig {
  return {
    internalSceneId,
    sceneIndex,
    sceneName,
    durationMs: 0,
    channelConfigs: [],
    scopedChannels: [],
    scopeToggles: { faders: true, pan: false },
  };
}

describe("findSmokeSceneConfigs", () => {
  test("finds smoke scenes by LV1 index and name when internal IDs are UUIDs", () => {
    const sceneA = scene("550e8400-e29b-41d4-a716-446655440000", 0, "Smoke A");
    const sceneB = scene("550e8400-e29b-41d4-a716-446655440001", 1, "Smoke B");

    expect(findSmokeSceneConfigs([sceneA, sceneB])).toEqual({ sceneA, sceneB });
  });
});

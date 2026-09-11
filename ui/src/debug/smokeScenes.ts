import type { SceneConfig } from "../types";

/**
 * @cc [owner:mixxorz,label:safety] smoke-scenes-require-exact-lv1-identity
 * Smoke scene resolution MUST match both the expected LV1 index and exact name; internal IDs or a
 * partial name match MUST NOT substitute for the console-owned scene identity.
 */
export function findSmokeSceneConfigs(sceneConfigs: SceneConfig[]) {
  return {
    sceneA: sceneConfigs.find(
      (scene) => scene.sceneIndex === 0 && scene.sceneName === "Smoke A",
    ),
    sceneB: sceneConfigs.find(
      (scene) => scene.sceneIndex === 1 && scene.sceneName === "Smoke B",
    ),
  };
}

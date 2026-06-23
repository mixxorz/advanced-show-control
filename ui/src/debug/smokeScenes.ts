import type { SceneConfig } from "../types";

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

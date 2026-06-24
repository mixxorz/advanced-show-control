import { formatSceneNumber } from "../format";
import type { SceneConfig, SceneSummary } from "../types";

export function SelectedSceneIdentity(props: {
  currentScene: SceneSummary | null;
  cued: boolean;
  scene: SceneConfig;
}) {
  const unlinked = props.scene.sceneIndex === null;
  const current =
    props.currentScene?.index === props.scene.sceneIndex &&
    props.currentScene.name === props.scene.sceneName;
  const textClass = unlinked
    ? "text-status-warning"
    : current
      ? "text-status-current"
      : props.cued
        ? "text-status-cued"
        : "text-console-primary";

  return (
    <div
      aria-label="Selected scene"
      className={`flex min-h-12 min-w-0 flex-1 items-center gap-3 rounded-console-panel border border-console-line bg-console-bg px-5 py-3 font-mono text-xl ${textClass}`}
    >
      <span className="text-accent-orange">
        {formatSceneNumber(props.scene.sceneIndex)}
      </span>{" "}
      <span className="font-ui">{props.scene.sceneName}</span>
    </div>
  );
}

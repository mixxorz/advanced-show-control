import type { SceneConfig, SceneSummary } from "../types";
import { formatSceneDurationSummary, formatSceneNumber } from "../format";

export function SceneListRow(props: {
  currentScene: SceneSummary | null;
  scene: SceneConfig;
  selected: boolean;
  onSelect: () => void;
}) {
  const current =
    props.currentScene?.index === props.scene.sceneIndex &&
    props.currentScene.name === props.scene.sceneName;

  return (
    <button
      className={
        props.selected
          ? "grid w-full grid-cols-[4rem_1fr_4rem] items-center border border-accent-orange-active bg-accent-orange-soft px-3 py-2 text-left text-console-primary"
          : "grid w-full grid-cols-[4rem_1fr_4rem] items-center border-b border-console-line-soft px-3 py-2 text-left text-console-secondary hover:bg-console-section hover:text-console-primary"
      }
      onClick={props.onSelect}
    >
      <span className={current ? "font-mono text-status-current" : "font-mono"}>
        {formatSceneNumber(props.scene.sceneIndex)}
      </span>
      <span className={current ? "truncate text-status-current" : "truncate"}>
        {props.scene.sceneName}
      </span>
      <span
        className={
          props.selected
            ? "text-right font-mono text-console-primary"
            : "text-right font-mono"
        }
      >
        {formatSceneDurationSummary(props.scene.durationMs)}
      </span>
    </button>
  );
}

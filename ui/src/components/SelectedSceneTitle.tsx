import type { SceneConfig } from "../types";
import { formatSceneNumber } from "../format";

export function SelectedSceneTitle(props: { scene: SceneConfig }) {
  return (
    <div className="min-w-0">
      <div className="text-lg font-normal uppercase text-accent-orange">
        Selected Scene
      </div>
      <h2 className="truncate font-mono text-3xl font-semibold leading-[2.75rem] text-console-primary">
        {formatSceneNumber(props.scene.sceneIndex)}{" "}
        <span className="font-ui">{props.scene.sceneName}</span>
      </h2>
    </div>
  );
}

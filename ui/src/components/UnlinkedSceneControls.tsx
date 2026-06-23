import { useState } from "react";
import type { SceneConfig, SceneSummary } from "../types";
import { useAppCommands } from "../appHooks";
import { ConsoleButton } from "./ConsoleButton";

function sceneIndexLabel(scene: SceneSummary) {
  return `${String(scene.index + 1).padStart(3, "0")} ${scene.name}`;
}

export function UnlinkedSceneControls(props: {
  scene: SceneConfig;
  lv1Scenes: SceneSummary[];
  existingConfigs: SceneConfig[];
}) {
  const commands = useAppCommands();
  const linkedSceneIndexes = new Set(
    props.existingConfigs
      .map((scene) => scene.sceneIndex)
      .filter((index) => index !== null),
  );
  const initialTargetIndex =
    props.lv1Scenes.find((scene) => !linkedSceneIndexes.has(scene.index))
      ?.index ??
    props.lv1Scenes[0]?.index ??
    null;
  const [selectedTargetIndex, setSelectedTargetIndex] = useState(
    String(initialTargetIndex ?? ""),
  );

  function linkSelectedTarget() {
    if (!selectedTargetIndex) return;
    const targetIndex = Number(selectedTargetIndex);
    const conflict = props.existingConfigs.some(
      (scene) => scene.sceneIndex === targetIndex,
    );
    if (conflict && !window.confirm("Overwrite existing linked scene?")) {
      return;
    }
    void commands.linkSceneConfig?.(
      props.scene.internalSceneId,
      targetIndex,
      conflict,
    );
  }

  function deleteUnlinkedScene() {
    void commands.deleteSceneConfig?.(props.scene.internalSceneId);
  }

  return (
    <div className="flex flex-wrap items-end gap-3 rounded-console-panel border border-console-line bg-console-panel p-4">
      <label className="flex flex-col gap-2 text-sm uppercase text-console-secondary">
        <span>LV1 Scene</span>
        <select
          aria-label="LV1 Scene"
          className="min-w-64 rounded-console-control border border-console-line bg-console-control px-3 py-2 text-console-primary outline-none"
          onChange={(event) => setSelectedTargetIndex(event.target.value)}
          value={selectedTargetIndex}
        >
          {props.lv1Scenes.map((scene) => (
            <option key={scene.index} value={scene.index}>
              {sceneIndexLabel(scene)}
            </option>
          ))}
        </select>
      </label>
      <ConsoleButton onClick={linkSelectedTarget} variant="ghost-primary">
        Link to LV1 Scene
      </ConsoleButton>
      <ConsoleButton onClick={deleteUnlinkedScene} variant="ghost-danger">
        Delete
      </ConsoleButton>
    </div>
  );
}

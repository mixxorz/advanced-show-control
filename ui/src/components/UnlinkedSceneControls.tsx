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
  const [pendingOverwriteTargetIndex, setPendingOverwriteTargetIndex] =
    useState<number | null>(null);
  const targetExists = props.lv1Scenes.some(
    (scene) => String(scene.index) === selectedTargetIndex,
  );
  const effectiveSelectedTargetIndex = targetExists
    ? selectedTargetIndex
    : String(initialTargetIndex ?? "");

  function linkSelectedTarget() {
    if (!effectiveSelectedTargetIndex) return;
    const targetIndex = Number(effectiveSelectedTargetIndex);
    const conflict = props.existingConfigs.some(
      (scene) => scene.sceneIndex === targetIndex,
    );
    if (conflict) {
      setPendingOverwriteTargetIndex(targetIndex);
      return;
    }
    linkTarget(targetIndex, false);
  }

  function linkTarget(targetIndex: number, overwriteExisting: boolean) {
    void commands.linkSceneConfig?.(
      props.scene.internalSceneId,
      targetIndex,
      overwriteExisting,
    );
  }

  function confirmOverwrite() {
    if (pendingOverwriteTargetIndex === null) return;
    linkTarget(pendingOverwriteTargetIndex, true);
    setPendingOverwriteTargetIndex(null);
  }

  function deleteUnlinkedScene() {
    void commands.deleteSceneConfig?.(props.scene.internalSceneId);
  }

  return (
    <>
      <div className="flex flex-wrap items-end gap-3 rounded-console-panel border border-console-line bg-console-panel p-4">
        <label className="flex flex-col gap-2 text-sm uppercase text-console-secondary">
          <span>LV1 Scene</span>
          <select
            aria-label="LV1 Scene"
            className="min-w-64 rounded-console-control border border-console-line bg-console-control px-3 py-2 text-console-primary outline-none"
            onChange={(event) => setSelectedTargetIndex(event.target.value)}
            value={effectiveSelectedTargetIndex}
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
      {pendingOverwriteTargetIndex !== null ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-6">
          <section
            aria-modal="true"
            className="max-w-md rounded-console-panel border border-console-line bg-console-panel p-6 shadow-2xl"
            role="dialog"
          >
            <h2 className="text-lg font-normal uppercase text-console-primary">
              Overwrite existing linked scene?
            </h2>
            <p className="mt-3 text-sm text-console-secondary">
              This will replace the current app config for LV1 scene{" "}
              {pendingOverwriteTargetIndex + 1} with the selected unlinked
              config.
            </p>
            <div className="mt-6 flex justify-end gap-3">
              <ConsoleButton
                onClick={() => setPendingOverwriteTargetIndex(null)}
                size="small"
                variant="ghost-secondary"
              >
                Cancel
              </ConsoleButton>
              <ConsoleButton
                onClick={confirmOverwrite}
                size="small"
                variant="ghost-danger"
              >
                Overwrite
              </ConsoleButton>
            </div>
          </section>
        </div>
      ) : null}
    </>
  );
}

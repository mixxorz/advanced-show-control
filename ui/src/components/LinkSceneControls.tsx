import { useState } from "react";
import type { SceneConfig, SceneSummary } from "../types";
import { useAppCommands } from "../appHooks";
import { ConsoleButton } from "./ConsoleButton";
import { Panel } from "./Panel";

function sceneIndexLabel(scene: SceneSummary) {
  return `${String(scene.index + 1).padStart(3, "0")} ${scene.name}`;
}

function defaultTargetIndex(
  lv1Scenes: SceneSummary[],
  existingConfigs: SceneConfig[],
) {
  return String(
    lv1Scenes.find(
      (scene) =>
        !existingConfigs.some((config) => config.sceneIndex === scene.index),
    )?.index ??
      lv1Scenes[0]?.index ??
      "",
  );
}

export function LinkSceneControls(props: {
  scene: SceneConfig;
  lv1Scenes: SceneSummary[];
  existingConfigs: SceneConfig[];
}) {
  const commands = useAppCommands();
  const fallbackTargetIndex = defaultTargetIndex(
    props.lv1Scenes,
    props.existingConfigs,
  );
  const [selectedTargetIndex, setSelectedTargetIndex] =
    useState(fallbackTargetIndex);
  const [pendingOverwriteTargetIndex, setPendingOverwriteTargetIndex] =
    useState<number | null>(null);
  const effectiveSelectedTargetIndex = props.lv1Scenes.some(
    (scene) => String(scene.index) === selectedTargetIndex,
  )
    ? selectedTargetIndex
    : fallbackTargetIndex;

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
      <Panel
        className="flex flex-wrap items-center justify-between gap-3 px-4 py-2"
        variant="warning"
      >
        <p className="text-base font-normal text-status-warning">
          Scene is currently unlinked
        </p>
        <div className="ml-auto flex flex-wrap items-center justify-end gap-3">
          <label className="flex shrink-0 items-center gap-3 text-sm font-normal uppercase text-console-secondary">
            <span>Scene</span>
            <div className="relative min-w-72">
              <select
                aria-label="LV1 Scene"
                className="w-full appearance-none rounded-console-control border border-console-line bg-console-panel px-3 py-1 pr-9 font-mono text-sm text-accent-orange outline-none transition-colors focus:border-console-line-strong"
                onChange={(event) => setSelectedTargetIndex(event.target.value)}
                value={effectiveSelectedTargetIndex}
              >
                {props.lv1Scenes.map((scene) => (
                  <option key={scene.index} value={scene.index}>
                    {sceneIndexLabel(scene)}
                  </option>
                ))}
              </select>
              <svg
                aria-hidden="true"
                className="pointer-events-none absolute top-1/2 right-3 h-2.5 w-2.5 -translate-y-1/2 stroke-white"
                fill="none"
                viewBox="0 0 12 12"
              >
                <path
                  d="M3 4.5 6 7.5l3-3"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth="2"
                />
              </svg>
            </div>
          </label>
          <ConsoleButton
            onClick={linkSelectedTarget}
            size="small"
            variant="primary"
          >
            Link to scene
          </ConsoleButton>
          <ConsoleButton
            onClick={deleteUnlinkedScene}
            size="small"
            variant="danger"
          >
            Delete
          </ConsoleButton>
        </div>
      </Panel>
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

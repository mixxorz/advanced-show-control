import { useEffect, useState } from "react";
import type { SceneConfig, SceneSummary } from "../types";
import { useAppCommands } from "../appHooks";
import { ConsoleButton } from "./ConsoleButton";
import { OverwriteSceneLinkModal } from "./OverwriteSceneLinkModal";
import { Panel } from "./Panel";

/**
 * @cc [owner:mixxorz,label:product] link-target-fallback-order
 * The default link target MUST be the first LV1 scene without an existing config, falling back to
 * the first LV1 scene and then to no target when the LV1 scene list is empty.
 */
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

/**
 * @cc [owner:mixxorz,label:product;safety] link-overwrite-gating
 * Linking MUST dispatch nothing unless the target index exists in the latest `lv1Scenes`. A target
 * currently claimed by a config MUST require explicit overwrite confirmation; an unclaimed target
 * MUST link with overwrite disabled.
 */
/**
 * @cc [owner:mixxorz,label:product] link-selection-and-command-sequencing
 * A selection absent from the latest `lv1Scenes` MUST fall back using `link-target-fallback-order`.
 * Pending overwrite intent MUST remain bound to the exact source internal ID and target index/name
 * that created it, and MUST be cleared when either identity ceases to match current props.
 * Confirmation MUST revalidate the latest conflict state, dispatch at most once using that conflict
 * as the overwrite flag, and clear the modal. Cancellation MUST clear it without dispatch. Delete
 * MUST target only the source scene's internal ID.
 */
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
  const [pendingOverwrite, setPendingOverwrite] = useState<{
    sourceInternalSceneId: string;
    targetIndex: number;
    targetName: string;
  } | null>(null);
  const effectiveSelectedTargetIndex = props.lv1Scenes.some(
    (scene) => String(scene.index) === selectedTargetIndex,
  )
    ? selectedTargetIndex
    : fallbackTargetIndex;

  const pendingTarget =
    pendingOverwrite?.sourceInternalSceneId === props.scene.internalSceneId
      ? props.lv1Scenes.find(
          (scene) =>
            scene.index === pendingOverwrite.targetIndex &&
            scene.name === pendingOverwrite.targetName,
        )
      : undefined;

  useEffect(() => {
    if (!pendingOverwrite || pendingTarget) return;

    queueMicrotask(() => {
      setPendingOverwrite((current) =>
        current === pendingOverwrite ? null : current,
      );
    });
  }, [pendingOverwrite, pendingTarget]);

  function linkSelectedTarget() {
    if (!effectiveSelectedTargetIndex) return;
    const target = props.lv1Scenes.find(
      (scene) => String(scene.index) === effectiveSelectedTargetIndex,
    );
    if (!target) return;

    const conflict = props.existingConfigs.some(
      (scene) => scene.sceneIndex === target.index,
    );
    if (conflict) {
      setPendingOverwrite({
        sourceInternalSceneId: props.scene.internalSceneId,
        targetIndex: target.index,
        targetName: target.name,
      });
      return;
    }
    linkTarget(props.scene.internalSceneId, target.index, false);
  }

  function linkTarget(
    sourceInternalSceneId: string,
    targetIndex: number,
    overwriteExisting: boolean,
  ) {
    void commands.linkSceneConfig(
      sourceInternalSceneId,
      targetIndex,
      overwriteExisting,
    );
  }

  function confirmOverwrite() {
    if (!pendingOverwrite || !pendingTarget) {
      setPendingOverwrite(null);
      return;
    }

    const conflictStillExists = props.existingConfigs.some(
      (scene) => scene.sceneIndex === pendingOverwrite.targetIndex,
    );
    linkTarget(
      pendingOverwrite.sourceInternalSceneId,
      pendingOverwrite.targetIndex,
      conflictStillExists,
    );
    setPendingOverwrite(null);
  }

  function deleteUnlinkedScene() {
    void commands.deleteSceneConfig(props.scene.internalSceneId);
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
                    {String(scene.index + 1).padStart(3, "0")} {scene.name}
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
      {pendingOverwrite && pendingTarget ? (
        <OverwriteSceneLinkModal
          onCancel={() => setPendingOverwrite(null)}
          onOverwrite={confirmOverwrite}
          sourceSceneName={props.scene.sceneName}
          targetSceneIndex={pendingOverwrite.targetIndex}
          targetSceneName={pendingTarget.name}
        />
      ) : null}
    </>
  );
}

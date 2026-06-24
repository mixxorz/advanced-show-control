import type { SceneConfig, SceneSummary } from "../types";
import { useAppCommands } from "../appHooks";
import { formatSceneNumber } from "../format";
import { ConsoleButton } from "./ConsoleButton";
import { DurationInput } from "./DurationInput";
import { SelectedSceneActions } from "./SelectedSceneActions";

export function SelectedSceneHeader(props: {
  currentScene: SceneSummary | null;
  cued: boolean;
  scene: SceneConfig;
}) {
  const commands = useAppCommands();
  const unlinked = props.scene.sceneIndex === null;
  const current =
    props.currentScene?.index === props.scene.sceneIndex &&
    props.currentScene.name === props.scene.sceneName;
  const identityTextClass = unlinked
    ? "text-status-warning"
    : current
      ? "text-status-current"
      : props.cued
        ? "text-status-cued"
        : "text-console-primary";

  return (
    <div className="flex flex-col gap-3">
      <div
        aria-label="Selected scene"
        className={`flex min-h-12 min-w-0 flex-1 items-center gap-3 rounded-console-panel border border-console-line bg-console-bg px-5 py-3 font-mono text-xl ${identityTextClass}`}
      >
        <span className="text-accent-orange">
          {formatSceneNumber(props.scene.sceneIndex)}
        </span>{" "}
        <span className="font-ui">{props.scene.sceneName}</span>
      </div>
      <div className="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
        <div className="flex flex-wrap items-end gap-3 md:flex-nowrap">
          <div className="flex flex-wrap items-end gap-3 md:flex-nowrap">
            <ConsoleButton
              disabled={unlinked}
              onClick={() =>
                commands.recallScene?.(props.scene.internalSceneId)
              }
              variant="ghost-primary"
            >
              Recall
            </ConsoleButton>
            <ConsoleButton
              disabled={unlinked}
              onClick={() => commands.cueScene?.(props.scene.internalSceneId)}
              variant="ghost-secondary"
            >
              Cue
            </ConsoleButton>
          </div>
          <div className="h-11 w-px bg-console-line" />
          <SelectedSceneActions scene={props.scene} />
        </div>
        <div className="flex flex-wrap items-end gap-3 md:flex-nowrap md:justify-end">
          <DurationInput
            durationMs={props.scene.durationMs}
            internalSceneId={props.scene.internalSceneId}
          />
        </div>
      </div>
    </div>
  );
}

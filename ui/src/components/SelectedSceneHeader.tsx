import type { SceneConfig, SceneSummary } from "../types";
import { useAppCommands } from "../appHooks";
import { ConsoleButton } from "./ConsoleButton";
import { DurationInput } from "./DurationInput";
import { SelectedSceneActions } from "./SelectedSceneActions";
import { SelectedSceneIdentity } from "./SelectedSceneIdentity";

export function SelectedSceneHeader(props: {
  currentScene: SceneSummary | null;
  cued: boolean;
  scene: SceneConfig;
}) {
  const commands = useAppCommands();
  const unlinked = props.scene.sceneIndex === null;

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-3 xl:flex-row xl:items-center xl:justify-between">
        <SelectedSceneIdentity
          currentScene={props.currentScene}
          cued={props.cued}
          scene={props.scene}
        />
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

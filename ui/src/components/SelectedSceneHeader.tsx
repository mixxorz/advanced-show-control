import type { SceneConfig } from "../types";
import { useAppCommands } from "../appHooks";
import { ConsoleButton } from "./ConsoleButton";
import { DurationInput } from "./DurationInput";
import { Panel } from "./Panel";
import { SelectedSceneActions } from "./SelectedSceneActions";

export function SelectedSceneHeader(props: { scene: SceneConfig }) {
  const commands = useAppCommands();

  return (
    <Panel className="p-4">
      <div className="flex flex-col gap-4 xl:flex-row xl:items-end xl:justify-between xl:gap-8">
        <div className="flex flex-wrap items-end gap-3 xl:flex-nowrap">
          <ConsoleButton
            onClick={() => commands.recallScene?.(props.scene.sceneId)}
            variant="ghost-primary"
          >
            Recall
          </ConsoleButton>
          <ConsoleButton
            onClick={() => commands.cueScene?.(props.scene.sceneId)}
            variant="ghost-secondary"
          >
            Cue
          </ConsoleButton>
        </div>
        <div className="flex flex-wrap items-end gap-3 xl:flex-nowrap">
          <SelectedSceneActions sceneId={props.scene.sceneId} />
        </div>
        <div className="flex flex-wrap items-end gap-3 xl:flex-nowrap xl:justify-end">
          <DurationInput
            durationMs={props.scene.durationMs}
            sceneId={props.scene.sceneId}
          />
        </div>
      </div>
    </Panel>
  );
}

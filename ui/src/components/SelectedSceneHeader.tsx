import type { SceneConfig } from "../types";
import { formatSceneNumber } from "../format";
import { ConsoleButton } from "./ConsoleButton";
import { DurationInput } from "./DurationInput";
import { Panel } from "./Panel";
import { ScopeToggleGroup } from "./ScopeToggleGroup";

export function SelectedSceneHeader(props: {
  scene: SceneConfig;
  onStore: () => void;
  onToggleFaders: () => void;
  onTogglePan: () => void;
  setSceneDurationMs: (sceneId: string, durationMs: number) => Promise<boolean>;
}) {
  return (
    <Panel className="p-4">
      <div className="flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between">
        <div className="min-w-0">
          <div className="text-xs font-semibold uppercase tracking-[0.12em] text-accent-orange">
            Selected Scene
          </div>
          <h2 className="mt-1 truncate font-mono text-2xl font-semibold text-console-primary">
            {formatSceneNumber(props.scene.sceneIndex)}{" "}
            <span className="font-ui">{props.scene.sceneName}</span>
          </h2>
        </div>
        <div className="flex flex-wrap items-end gap-4 xl:justify-end">
          <div>
            <div className="mb-2 text-xs uppercase tracking-[0.12em] text-console-secondary">
              Scene Scope
            </div>
            <ScopeToggleGroup
              fadersEnabled={props.scene.scopeToggles.faders}
              onToggleFaders={props.onToggleFaders}
              onTogglePan={props.onTogglePan}
              panEnabled={props.scene.scopeToggles.pan}
            />
          </div>
          <DurationInput
            durationMs={props.scene.durationMs}
            sceneId={props.scene.sceneId}
            setSceneDurationMs={props.setSceneDurationMs}
          />
          <div className="flex items-center gap-2">
            <ConsoleButton onClick={props.onStore} variant="primary">
              Store
            </ConsoleButton>
            <ConsoleButton disabled>Cue</ConsoleButton>
            <ConsoleButton disabled>Recall</ConsoleButton>
          </div>
        </div>
      </div>
    </Panel>
  );
}

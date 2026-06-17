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
      <div className="grid gap-6 xl:grid-cols-[1fr_auto_auto] xl:items-center">
        <div>
          <div className="text-sm font-semibold uppercase tracking-[0.08em] text-accent-orange">
            Selected Scene
          </div>
          <h2 className="mt-2 font-mono text-3xl font-semibold text-console-primary">
            {formatSceneNumber(props.scene.sceneIndex)}{" "}
            <span className="font-ui">{props.scene.sceneName}</span>
          </h2>
        </div>
        <div>
          <div className="mb-2 text-sm uppercase tracking-[0.08em] text-console-secondary">
            Scene Scope
          </div>
          <ScopeToggleGroup
            fadersEnabled={props.scene.scopeToggles.faders}
            onToggleFaders={props.onToggleFaders}
            onTogglePan={props.onTogglePan}
            panEnabled={props.scene.scopeToggles.pan}
          />
        </div>
        <div className="flex flex-wrap items-end gap-3">
          <DurationInput
            durationMs={props.scene.durationMs}
            sceneId={props.scene.sceneId}
            setSceneDurationMs={props.setSceneDurationMs}
          />
          <ConsoleButton onClick={props.onStore} variant="primary">
            Store
          </ConsoleButton>
          <ConsoleButton disabled>Cue</ConsoleButton>
          <ConsoleButton disabled>Recall</ConsoleButton>
        </div>
      </div>
    </Panel>
  );
}

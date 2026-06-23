import { useAppCommands } from "../appHooks";
import type { SceneConfig } from "../types";
import { ScopeToggleGroup } from "./ScopeToggleGroup";

export function SceneScopeControls(props: {
  internalSceneId: string;
  scopeToggles: SceneConfig["scopeToggles"];
}) {
  const commands = useAppCommands();

  return (
    <div className="shrink-0">
      <div className="mb-2 text-base font-normal uppercase text-console-secondary">
        Scene Scope
      </div>
      <ScopeToggleGroup
        fadersEnabled={props.scopeToggles.faders}
        onToggleFaders={() =>
          commands.setSceneScopeFadersEnabled(
            props.internalSceneId,
            !props.scopeToggles.faders,
          )
        }
        onTogglePan={() =>
          commands.setSceneScopePanEnabled(
            props.internalSceneId,
            !props.scopeToggles.pan,
          )
        }
        panEnabled={props.scopeToggles.pan}
      />
    </div>
  );
}

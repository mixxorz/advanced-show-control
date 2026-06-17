import { useAppCommands } from "../appHooks";
import type { SceneConfig } from "../types";
import { ConsoleButton } from "./ConsoleButton";
import { ScopeToggleGroup } from "./ScopeToggleGroup";

export function ChannelScopeToolbar(props: {
  allChannelsScoped: boolean;
  noChannelsScoped: boolean;
  sceneId: string;
  scopeToggles: SceneConfig["scopeToggles"];
}) {
  const commands = useAppCommands();

  return (
    <div className="flex flex-wrap items-center justify-between gap-3 border-b border-console-line pb-3">
      <div className="flex items-center gap-5">
        <h3 className="text-lg font-normal uppercase text-console-primary">
          Scope
        </h3>
        <ScopeToggleGroup
          fadersEnabled={props.scopeToggles.faders}
          onToggleFaders={() =>
            commands.setSceneScopeFadersEnabled(
              props.sceneId,
              !props.scopeToggles.faders,
            )
          }
          onTogglePan={() =>
            commands.setSceneScopePanEnabled(
              props.sceneId,
              !props.scopeToggles.pan,
            )
          }
          panEnabled={props.scopeToggles.pan}
          size="small"
        />
      </div>
      <div className="flex gap-2">
        <ConsoleButton
          active={props.allChannelsScoped}
          onClick={() => commands.setAllChannelsScoped(props.sceneId, true)}
          size="small"
        >
          All
        </ConsoleButton>
        <ConsoleButton
          active={props.noChannelsScoped}
          onClick={() => commands.setAllChannelsScoped(props.sceneId, false)}
          size="small"
        >
          None
        </ConsoleButton>
      </div>
    </div>
  );
}

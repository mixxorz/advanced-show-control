import { useAppCommands } from "../appHooks";
import type { SceneConfig } from "../types";
import { ConsoleButton } from "./ConsoleButton";
import { ScopeToggleGroup } from "./ScopeToggleGroup";

/**
 * @cc [owner:mixxorz,label:product] scope-toolbar-command-mapping
 * Fader and pan controls MUST request the inverse of their corresponding projected toggle for the
 * supplied scene. `All` and `None` MUST request `setAllChannelsScoped` with `true` and `false`
 * respectively, and their active states MUST come only from the aggregate props.
 */
export function ChannelScopeToolbar(props: {
  allChannelsScoped: boolean;
  noChannelsScoped: boolean;
  internalSceneId: string;
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
          size="small"
        />
      </div>
      <div className="flex gap-2">
        <ConsoleButton
          active={props.allChannelsScoped}
          onClick={() =>
            commands.setAllChannelsScoped(props.internalSceneId, true)
          }
          size="small"
        >
          All
        </ConsoleButton>
        <ConsoleButton
          active={props.noChannelsScoped}
          onClick={() =>
            commands.setAllChannelsScoped(props.internalSceneId, false)
          }
          size="small"
        >
          None
        </ConsoleButton>
      </div>
    </div>
  );
}

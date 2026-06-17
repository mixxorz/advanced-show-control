import { useAppCommands, useAppState } from "../appHooks";
import type { ChannelConfig } from "../types";
import {
  channelButtonLabel,
  channelName,
  formatDb,
  formatPanFamilySummary,
} from "../format";
import { ScopeButton } from "./ScopeButton";

export function ChannelScopeButton(props: {
  config: ChannelConfig;
  sceneId: string;
  scoped: boolean;
}) {
  const { appState } = useAppState();
  const commands = useAppCommands();

  return (
    <ScopeButton
      active={props.scoped}
      label={channelButtonLabel(props.config.group, props.config.channel)}
      onClick={() =>
        commands.setChannelScoped(
          props.sceneId,
          props.config.group,
          props.config.channel,
          !props.scoped,
        )
      }
      title={`${channelName(appState.channels, props.config.group, props.config.channel)} · ${formatDb(props.config.faderDb ?? 0)} · ${formatPanFamilySummary(props.config)}`}
    />
  );
}

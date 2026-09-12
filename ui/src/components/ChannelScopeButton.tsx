import { useAppCommands, useAppState } from "../appHooks";
import type { ChannelConfig } from "../types";
import {
  channelButtonLabel,
  channelName,
  formatDb,
  formatPanFamilySummary,
} from "../format";
import { ScopeButton } from "./ScopeButton";

/**
 * @cc [owner:mixxorz,label:product] channel-scope-toggle-command
 * Activating the button MUST request the opposite of `scoped` for exactly the supplied scene,
 * channel group, and channel; visual active state MUST continue to reflect `scoped`.
 */
/**
 * @cc [owner:mixxorz,label:product;accessibility] channel-scope-label-and-summary
 * `config` MUST identify a stored channel config. The visible label MUST use LV1's channel-family
 * numbering. The native `title` MUST include the projected channel-name fallback, a `0.0 dB`
 * fallback when stored fader data is absent, and the available pan-family values.
 */
export function ChannelScopeButton(props: {
  config: ChannelConfig;
  internalSceneId: string;
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
          props.internalSceneId,
          props.config.group,
          props.config.channel,
          !props.scoped,
        )
      }
      title={`${channelName(appState.channels, props.config.group, props.config.channel)} · ${formatDb(props.config.faderDb ?? 0)} · ${formatPanFamilySummary(props.config)}`}
    />
  );
}

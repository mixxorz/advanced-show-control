import type { ChannelConfig, SceneConfig } from "../types";
import { channelDisplayGroup, channelDisplayGroupOrder } from "../format";
import { ChannelScopeEmptyState } from "./ChannelScopeEmptyState";
import { ChannelScopeGroup } from "./ChannelScopeGroup";
import { ChannelScopeToolbar } from "./ChannelScopeToolbar";
import { Panel } from "./Panel";

/**
 * @cc [owner:mixxorz,label:product] scope-grid-stored-config-gate
 * When `scene.channelConfigs` is empty, the grid MUST render the non-interactive store-first state
 * and MUST NOT expose channel or bulk scope controls.
 */
/**
 * @cc [owner:mixxorz,label:product] scope-grid-membership-and-aggregate-state
 * Scope membership MUST match exact `(group, channel)` pairs. `All` MUST be active only when every
 * stored config has a matching scoped pair, regardless of duplicate or extraneous scoped entries;
 * `None` MUST be active only when there are no scoped pairs.
 */
export function ChannelScopeGrid(props: { scene: SceneConfig }) {
  const scoped = new Set(
    props.scene.scopedChannels.map(
      (entry) => `${entry.group}:${entry.channel}`,
    ),
  );
  const groups = new Map<string, ChannelConfig[]>();

  for (const config of props.scene.channelConfigs) {
    const groupName = channelDisplayGroup(config.group);
    groups.set(groupName, [...(groups.get(groupName) ?? []), config]);
  }

  const grouped = [...groups.entries()].sort(
    ([a], [b]) => channelDisplayGroupOrder(a) - channelDisplayGroupOrder(b),
  );

  if (props.scene.channelConfigs.length === 0) {
    return <ChannelScopeEmptyState />;
  }

  return (
    <Panel className="flex min-h-0 flex-1 flex-col overflow-hidden p-4">
      <ChannelScopeToolbar
        allChannelsScoped={props.scene.channelConfigs.every((config) =>
          scoped.has(`${config.group}:${config.channel}`),
        )}
        noChannelsScoped={scoped.size === 0}
        internalSceneId={props.scene.internalSceneId}
        scopeToggles={props.scene.scopeToggles}
      />
      <div className="mt-3 min-h-0 flex-1 space-y-3 overflow-auto">
        {grouped.map(([groupName, configs]) => (
          <ChannelScopeGroup
            configs={configs}
            groupName={groupName}
            key={groupName}
            internalSceneId={props.scene.internalSceneId}
            scoped={scoped}
          />
        ))}
      </div>
    </Panel>
  );
}

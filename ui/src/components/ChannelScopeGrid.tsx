import type { AppViewState, ChannelConfig, SceneConfig } from "../types";
import {
  channelButtonLabel,
  channelDisplayGroup,
  channelDisplayGroupOrder,
  channelName,
  formatDb,
  formatPanFamilySummary,
} from "../format";
import { ConsoleButton } from "./ConsoleButton";
import { Panel } from "./Panel";
import { ScopeButton } from "./ScopeButton";

function channelKey(group: number, channel: number) {
  return `${group}:${channel}`;
}

export function ChannelScopeGrid(props: {
  channels: AppViewState["channels"];
  scene: SceneConfig;
  setChannelScoped: (
    sceneId: string,
    group: number,
    channel: number,
    scoped: boolean,
  ) => void;
  setAllChannelsScoped: (sceneId: string, scoped: boolean) => void;
}) {
  const scoped = new Set(
    props.scene.scopedChannels.map((entry) =>
      channelKey(entry.group, entry.channel),
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
    return (
      <Panel className="p-4 text-sm text-console-muted">
        Store the current mixer state to choose scoped channels.
      </Panel>
    );
  }

  return (
    <Panel className="p-4">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-console-line pb-3">
        <h3 className="text-sm font-semibold uppercase tracking-[0.12em] text-console-primary">
          Channel Scope
        </h3>
        <div className="flex gap-2">
          <ConsoleButton
            onClick={() =>
              props.setAllChannelsScoped(props.scene.sceneId, true)
            }
          >
            All
          </ConsoleButton>
          <ConsoleButton
            onClick={() =>
              props.setAllChannelsScoped(props.scene.sceneId, false)
            }
          >
            None
          </ConsoleButton>
        </div>
      </div>
      <div className="mt-3 space-y-3">
        {grouped.map(([groupName, configs]) => (
          <section
            className="rounded-console-panel border border-console-line-soft bg-console-section p-2.5"
            key={groupName}
          >
            <h4 className="text-[11px] font-semibold uppercase tracking-[0.12em] text-console-secondary">
              {groupName}
            </h4>
            <div className="mt-2 flex flex-wrap gap-1.5">
              {[...configs]
                .sort((a, b) => a.channel - b.channel)
                .map((config) => {
                  const key = channelKey(config.group, config.channel);
                  const isScoped = scoped.has(key);

                  return (
                    <ScopeButton
                      active={isScoped}
                      key={key}
                      label={channelButtonLabel(config.group, config.channel)}
                      onClick={() =>
                        props.setChannelScoped(
                          props.scene.sceneId,
                          config.group,
                          config.channel,
                          !isScoped,
                        )
                      }
                      title={`${channelName(props.channels, config.group, config.channel)} · ${formatDb(config.faderDb ?? 0)} · ${formatPanFamilySummary(config)}`}
                    />
                  );
                })}
            </div>
          </section>
        ))}
      </div>
    </Panel>
  );
}

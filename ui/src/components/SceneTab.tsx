import type { AppViewState, ChannelConfig, SceneConfig } from "../types";
import {
  channelButtonLabel,
  channelDisplayGroup,
  channelDisplayGroupOrder,
  channelName,
  formatDb,
} from "../format";
import { DurationInput } from "./DurationInput";

export function SceneTab(props: {
  appState: AppViewState;
  selectScene: (sceneId: string) => void;
  setSceneDurationMs: (sceneId: string, durationMs: number) => Promise<boolean>;
  storeSceneConfig: (sceneId: string) => Promise<boolean>;
  setChannelScoped: (sceneId: string, group: number, channel: number, scoped: boolean) => void;
  setAllChannelsScoped: (sceneId: string, scoped: boolean) => void;
}) {
  const selected = props.appState.sceneConfigs.find((scene) => scene.sceneId === props.appState.selectedSceneId);

  return (
    <div className="grid gap-5 lg:grid-cols-[22rem_1fr]">
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <h2 className="text-lg font-semibold">Scenes</h2>
        <p className="mt-1 text-sm text-slate-400">
          Select the scene config to edit.
        </p>
        <div className="mt-4 max-h-[34rem] overflow-auto rounded-lg border border-slate-800">
          {props.appState.sceneConfigs.length === 0 ? (
            <p className="p-3 text-sm text-slate-400">No scenes loaded.</p>
          ) : (
            props.appState.sceneConfigs.map((scene) => {
              const selectedRow = scene.sceneId === props.appState.selectedSceneId;

              return (
                <button
                  className={
                    selectedRow
                      ? "block w-full border-b border-slate-800 bg-cyan-950/40 px-3 py-3 text-left last:border-b-0"
                    : "block w-full border-b border-slate-800 px-3 py-3 text-left hover:bg-slate-800 disabled:cursor-not-allowed disabled:opacity-50 last:border-b-0"
                  }
                  key={scene.sceneId}
                  onClick={() => props.selectScene(scene.sceneId)}
                >
                  <span className="block text-sm font-semibold text-slate-100">
                    {scene.sceneIndex}: {scene.sceneName}
                  </span>
                  <span className="mt-1 block text-xs text-slate-400">
                    {scene.durationMs > 0 ? `${scene.durationMs} ms` : "Disabled"} · {scene.scopedChannels.length}/{scene.channelConfigs.length} scoped
                  </span>
                </button>
              );
            })
          )}
        </div>
      </section>
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        {selected ? (
          <div>
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div>
                <h2 className="text-lg font-semibold">
                  {selected.sceneIndex}: {selected.sceneName}
                </h2>
                <p className="mt-1 text-sm text-slate-400">Current LV1 scene does not affect which scene config is edited.</p>
              </div>
              <div className="flex flex-wrap gap-3">
                <button
                  className="rounded-lg bg-cyan-700 px-4 py-2 font-bold text-white hover:bg-cyan-600"
                  onClick={() => props.storeSceneConfig(selected.sceneId)}
                >
                  Store
                </button>
              </div>
            </div>
            <DurationInput
              durationMs={selected.durationMs}
              sceneId={selected.sceneId}
              setSceneDurationMs={props.setSceneDurationMs}
            />

            <ScopeGrid
              channels={props.appState.channels}
              scene={selected}
              setAllChannelsScoped={props.setAllChannelsScoped}
              setChannelScoped={props.setChannelScoped}
            />
          </div>
        ) : (
          <p className="text-sm text-slate-400">Select a scene to edit its scoped channels.</p>
        )}
      </section>
    </div>
  );
}

function channelKey(group: number, channel: number) {
  return `${group}:${channel}`;
}

function ScopeGrid(props: {
  channels: AppViewState["channels"];
  scene: SceneConfig;
  setChannelScoped: (sceneId: string, group: number, channel: number, scoped: boolean) => void;
  setAllChannelsScoped: (sceneId: string, scoped: boolean) => void;
}) {
  const scoped = new Set(props.scene.scopedChannels.map((entry) => channelKey(entry.group, entry.channel)));
  const groups = new Map<string, ChannelConfig[]>();

  for (const config of props.scene.channelConfigs) {
    const groupName = channelDisplayGroup(config.group);
    groups.set(groupName, [...(groups.get(groupName) ?? []), config]);
  }

  const grouped = [...groups.entries()].sort(([a], [b]) => channelDisplayGroupOrder(a) - channelDisplayGroupOrder(b));

  if (props.scene.channelConfigs.length === 0) {
    return <p className="mt-5 rounded-lg border border-slate-800 p-4 text-sm text-slate-400">Store the current mixer state to choose scoped channels.</p>;
  }

  return (
    <div className="mt-5 rounded-lg border border-slate-800 p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h3 className="font-semibold text-slate-100">Scoped Channels</h3>
        <div className="flex gap-2">
          <button className="rounded border border-slate-700 px-3 py-1 text-sm hover:bg-slate-800" onClick={() => props.setAllChannelsScoped(props.scene.sceneId, true)}>
            All
          </button>
          <button className="rounded border border-slate-700 px-3 py-1 text-sm hover:bg-slate-800" onClick={() => props.setAllChannelsScoped(props.scene.sceneId, false)}>
            None
          </button>
        </div>
      </div>
      <div className="mt-4 space-y-4">
        {grouped.map(([groupName, configs]) => (
          <section key={groupName}>
            <h4 className="text-xs font-semibold uppercase tracking-wide text-slate-400">{groupName}</h4>
            <div className="mt-2 flex flex-wrap gap-2">
              {configs
                .sort((a, b) => a.channel - b.channel)
                .map((config) => {
                  const key = channelKey(config.group, config.channel);
                  const isScoped = scoped.has(key);

                  return (
                    <button
                      className={
                        isScoped
                          ? "rounded bg-cyan-700 px-3 py-2 text-sm font-bold text-white"
                          : "rounded bg-slate-800 px-3 py-2 text-sm font-bold text-slate-300 hover:bg-slate-700"
                      }
                      key={key}
                      onClick={() => props.setChannelScoped(props.scene.sceneId, config.group, config.channel, !isScoped)}
                      title={`${channelName(props.channels, config.group, config.channel)} · ${formatDb(config.faderDb ?? 0)}`}
                    >
                      {channelButtonLabel(config.group, config.channel)}
                    </button>
                  );
                })}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}

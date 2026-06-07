import type { AppViewState, ChannelSummary, SceneFadeConfig } from "../types";
import { channelName, formatDb } from "../format";
import { DurationInput } from "./DurationInput";

export function SceneTab(props: {
  appState: AppViewState;
  selectScene: (sceneId: string) => void;
  setSceneFadeEnabled: (sceneId: string, enabled: boolean) => void;
  setSceneDurationMs: (sceneId: string, durationMs: number) => Promise<boolean>;
  setListenMode: (active: boolean) => void;
  setFadeTargetEnabled: (sceneId: string, group: number, channel: number, enabled: boolean) => void;
  removeFadeTarget: (sceneId: string, group: number, channel: number) => void;
}) {
  const selected = props.appState.sceneFadeConfigs.find((scene) => scene.sceneId === props.appState.selectedSceneId);

  return (
    <div className="grid gap-5 lg:grid-cols-[22rem_1fr]">
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <h2 className="text-lg font-semibold">Scenes</h2>
        <p className="mt-1 text-sm text-slate-400">
          Select the scene fade config to edit. Scene selection locks while Listen Mode is active.
        </p>
        <div className="mt-4 max-h-[34rem] overflow-auto rounded-lg border border-slate-800">
          {props.appState.sceneFadeConfigs.length === 0 ? (
            <p className="p-3 text-sm text-slate-400">No scenes loaded.</p>
          ) : (
            props.appState.sceneFadeConfigs.map((scene) => {
              const selectedRow = scene.sceneId === props.appState.selectedSceneId;

              return (
                <button
                  className={
                    selectedRow
                      ? "block w-full border-b border-slate-800 bg-cyan-950/40 px-3 py-3 text-left last:border-b-0"
                      : "block w-full border-b border-slate-800 px-3 py-3 text-left hover:bg-slate-800 disabled:cursor-not-allowed disabled:opacity-50 last:border-b-0"
                  }
                  disabled={props.appState.listenModeActive}
                  key={scene.sceneId}
                  onClick={() => props.selectScene(scene.sceneId)}
                >
                  <span className="block text-sm font-semibold text-slate-100">
                    {scene.sceneIndex}: {scene.sceneName}
                  </span>
                  <span className="mt-1 block text-xs text-slate-400">
                    {scene.fadeEnabled ? "Enabled" : "Disabled"} · {scene.fadeTargets.length} targets
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
                  className={
                    selected.fadeEnabled
                      ? "rounded-lg border border-emerald-500/60 bg-emerald-950 px-4 py-2 font-semibold text-emerald-100"
                      : "rounded-lg border border-slate-700 px-4 py-2 font-semibold text-slate-100 hover:bg-slate-800"
                  }
                  onClick={() => props.setSceneFadeEnabled(selected.sceneId, !selected.fadeEnabled)}
                >
                  {selected.fadeEnabled ? "Fade Enabled" : "Fade Disabled"}
                </button>
                <button
                  className={
                    props.appState.listenModeActive
                      ? "rounded-lg bg-amber-700 px-4 py-2 font-bold text-white hover:bg-amber-600"
                      : "rounded-lg bg-cyan-700 px-4 py-2 font-bold text-white hover:bg-cyan-600"
                  }
                  onClick={() => props.setListenMode(!props.appState.listenModeActive)}
                >
                  {props.appState.listenModeActive ? "Stop Listen Mode" : "Start Listen Mode"}
                </button>
              </div>
            </div>
            <DurationInput
              durationMs={selected.durationMs}
              sceneId={selected.sceneId}
              setSceneDurationMs={props.setSceneDurationMs}
            />

            <FadeTargetTable
              channels={props.appState.channels}
              removeFadeTarget={props.removeFadeTarget}
              scene={selected}
              setFadeTargetEnabled={props.setFadeTargetEnabled}
            />
          </div>
        ) : (
          <p className="text-sm text-slate-400">Select a scene to edit its fade targets.</p>
        )}
      </section>
    </div>
  );
}

function FadeTargetTable(props: {
  channels: ChannelSummary[];
  scene: SceneFadeConfig;
  setFadeTargetEnabled: (sceneId: string, group: number, channel: number, enabled: boolean) => void;
  removeFadeTarget: (sceneId: string, group: number, channel: number) => void;
}) {
  return (
    <div className="mt-5 overflow-auto rounded-lg border border-slate-800">
      {props.scene.fadeTargets.length === 0 ? (
        <p className="p-3 text-sm text-slate-400">No fader targets captured. Start Listen Mode and move LV1 faders.</p>
      ) : (
        <table className="w-full min-w-[42rem] text-sm">
          <thead className="bg-slate-950 text-left text-slate-400">
            <tr>
              <th className="px-3 py-2">Include</th>
              <th className="px-3 py-2">Group</th>
              <th className="px-3 py-2">Channel</th>
              <th className="px-3 py-2">Name</th>
              <th className="px-3 py-2">Target</th>
              <th className="px-3 py-2">Updated</th>
              <th className="px-3 py-2">Action</th>
            </tr>
          </thead>
          <tbody>
            {props.scene.fadeTargets.map((target) => (
              <tr className="border-t border-slate-800" key={`${target.group}-${target.channel}`}>
                <td className="px-3 py-2">
                  <input
                    checked={target.enabled}
                    onChange={(event) =>
                      props.setFadeTargetEnabled(props.scene.sceneId, target.group, target.channel, event.target.checked)
                    }
                    type="checkbox"
                  />
                </td>
                <td className="px-3 py-2">{target.group}</td>
                <td className="px-3 py-2">{target.channel}</td>
                <td className="px-3 py-2">
                  <div className="font-medium text-slate-100">{target.channelName}</div>
                  <div className="text-xs text-slate-400">
                    Current: {channelName(props.channels, target.group, target.channel)}
                  </div>
                </td>
                <td className="px-3 py-2">{formatDb(target.targetDb)}</td>
                <td className="px-3 py-2 text-slate-400">{target.updatedAt}</td>
                <td className="px-3 py-2">
                  <button
                    className="rounded border border-red-800 px-3 py-1 text-red-100 hover:bg-red-950"
                    onClick={() => props.removeFadeTarget(props.scene.sceneId, target.group, target.channel)}
                  >
                    Remove
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

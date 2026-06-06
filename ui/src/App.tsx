import { type ReactNode, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { disconnectedAppViewState, type AppViewState, type ChannelSummary, type SceneFadeConfig } from "./types";

type Tab = "connection" | "scene" | "logs";

export default function App() {
  const [activeTab, setActiveTab] = useState<Tab>("connection");
  const [host, setHost] = useState("");
  const [port, setPort] = useState("");
  const [commandError, setCommandError] = useState<string | null>(null);
  const [appState, setAppState] = useState<AppViewState>(disconnectedAppViewState);

  useEffect(() => {
    let cancelled = false;
    void refreshAppState(setAppState, setCommandError);

    const unlistenPromise = listen<AppViewState>("app-status-changed", (event) => {
      if (cancelled) {
        return;
      }
      setAppState(event.payload);
    });

    return () => {
      cancelled = true;
      void unlistenPromise.then((unlisten) => {
        void unlisten();
      });
    };
  }, []);

  async function runSnapshotCommand(command: string, args?: Record<string, unknown>) {
    setCommandError(null);
    try {
      const next = await invoke<AppViewState>(command, args);
      setAppState(next);
    } catch (error) {
      setCommandError(String(error));
      await refreshAppState(setAppState, setCommandError);
    }
  }

  async function runVoidCommand(command: string) {
    setCommandError(null);
    try {
      await invoke(command);
      await refreshAppState(setAppState, setCommandError);
    } catch (error) {
      setCommandError(String(error));
    }
  }

  async function connect() {
    const args: { host?: string; port?: number } = {};

    if (host.trim()) {
      args.host = host.trim();
    }

    const parsedPort = Number(port);
    if (Number.isInteger(parsedPort) && parsedPort > 0) {
      args.port = parsedPort;
    }

    await runSnapshotCommand("connect_lv1", args);
  }

  return (
    <main className="min-h-screen bg-slate-950 text-slate-100">
      <header className="border-b border-slate-800 bg-slate-900/80 px-6 py-4">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold">LV1 Scene Fade Utility</h1>
            <p className="text-sm text-slate-400">
              {appState.currentScene
                ? `Scene ${appState.currentScene.index}: ${appState.currentScene.name}`
                : "No LV1 scene selected"}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <ShowFileControls
              dirty={appState.showFileDirty}
              fileName={appState.showFileName}
              filePath={appState.showFilePath}
              onNew={() => runSnapshotCommand("new_show_file")}
              onOpen={() => runSnapshotCommand("open_show_file_dialog")}
              onSave={() => runSnapshotCommand("save_show_file")}
              onSaveAs={() => runSnapshotCommand("save_show_file_as_dialog")}
            />
            <StatusBadge
              label={appState.connection}
              tone={appState.connection === "connected" ? "good" : "neutral"}
            />
            <StatusBadge
              label={`Fade: ${appState.fadeState}`}
              tone={appState.fadeState === "blocked" ? "warning" : "neutral"}
            />
            <button
              className={
                appState.lockout
                  ? "rounded-full border border-amber-500/60 bg-amber-950 px-3 py-1 text-sm text-amber-100"
                  : "rounded-full border border-slate-700 bg-slate-800 px-3 py-1 text-sm text-slate-200"
              }
              onClick={() => runSnapshotCommand("set_lockout", { enabled: !appState.lockout })}
            >
              {appState.lockout ? "Lockout On" : "Lockout Off"}
            </button>
            <button
              className="rounded-lg border border-slate-700 px-4 py-3 font-semibold text-slate-100 hover:bg-slate-800"
              onClick={() => runVoidCommand("finish_fade_now")}
            >
              Finish Now
            </button>
            <button
              className="rounded-lg bg-red-700 px-5 py-3 font-bold text-white shadow-lg shadow-red-950/40 hover:bg-red-600"
              onClick={() => runVoidCommand("abort_all_fades")}
            >
              Abort All
            </button>
          </div>
        </div>
        {commandError && (
          <p className="mt-3 rounded-lg border border-red-800 bg-red-950 px-3 py-2 text-sm text-red-100">
            {commandError}
          </p>
        )}
      </header>

      <nav className="border-b border-slate-800 px-6">
        <div className="flex gap-2">
          <TabButton active={activeTab === "connection"} onClick={() => setActiveTab("connection")}>
            Connection
          </TabButton>
          <TabButton active={activeTab === "scene"} onClick={() => setActiveTab("scene")}>
            Scene
          </TabButton>
          <TabButton active={activeTab === "logs"} onClick={() => setActiveTab("logs")}>
            Logs
          </TabButton>
        </div>
      </nav>

      <section className="p-6">
        {activeTab === "connection" && (
          <ConnectionTab
            host={host}
            port={port}
            appState={appState}
            setHost={setHost}
            setPort={setPort}
            connect={connect}
            disconnect={() => runSnapshotCommand("disconnect_lv1")}
          />
        )}
        {activeTab === "scene" && (
          <SceneTab
            appState={appState}
            selectScene={(sceneId) => runSnapshotCommand("select_scene_config", { sceneId })}
            setSceneFadeEnabled={(sceneId, enabled) =>
              runSnapshotCommand("set_scene_fade_enabled", { sceneId, enabled })
            }
            setSceneDurationMs={(sceneId, durationMs) =>
              runSnapshotCommand("set_scene_duration_ms", { sceneId, durationMs })
            }
            setListenMode={(active) => runSnapshotCommand("set_listen_mode", { active })}
            setFadeTargetEnabled={(sceneId, group, channel, enabled) =>
              runSnapshotCommand("set_fade_target_enabled", { sceneId, group, channel, enabled })
            }
            removeFadeTarget={(sceneId, group, channel) =>
              runSnapshotCommand("remove_fade_target", { sceneId, group, channel })
            }
          />
        )}
        {activeTab === "logs" && <LogsTab appState={appState} />}
      </section>
    </main>
  );
}

function TabButton(props: { active: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      className={
        props.active
          ? "border-b-2 border-cyan-400 px-4 py-3 text-cyan-200"
          : "px-4 py-3 text-slate-400 hover:text-slate-100"
      }
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}

function StatusBadge(props: { label: string; tone: "neutral" | "warning" | "good" }) {
  const tone =
    props.tone === "warning"
      ? "border-amber-500/60 bg-amber-950 text-amber-100"
      : props.tone === "good"
        ? "border-emerald-500/60 bg-emerald-950 text-emerald-100"
        : "border-slate-700 bg-slate-800 text-slate-200";

  return <span className={`rounded-full border px-3 py-1 text-sm ${tone}`}>{props.label}</span>;
}

function ShowFileControls(props: {
  dirty: boolean;
  fileName: string;
  filePath: string | null;
  onNew: () => void;
  onOpen: () => void;
  onSave: () => void;
  onSaveAs: () => void;
}) {
  return (
    <div className="rounded-xl border border-slate-800 bg-slate-950/60 px-4 py-3">
      <div className="text-sm font-semibold text-slate-100">
        {props.fileName}
        {props.dirty ? " *" : ""}
      </div>
      <div className="mt-1 text-xs text-slate-400">{props.filePath ?? "No show file saved"}</div>
      <div className="mt-3 flex flex-wrap gap-2">
        <button
          className="rounded border border-slate-700 px-3 py-1 text-sm text-slate-100 hover:bg-slate-800"
          onClick={props.onNew}
        >
          New
        </button>
        <button
          className="rounded border border-slate-700 px-3 py-1 text-sm text-slate-100 hover:bg-slate-800"
          onClick={props.onOpen}
        >
          Open
        </button>
        <button
          className="rounded border border-slate-700 px-3 py-1 text-sm text-slate-100 hover:bg-slate-800"
          onClick={props.onSave}
        >
          Save
        </button>
        <button
          className="rounded border border-slate-700 px-3 py-1 text-sm text-slate-100 hover:bg-slate-800"
          onClick={props.onSaveAs}
        >
          Save As
        </button>
      </div>
    </div>
  );
}

function ConnectionTab(props: {
  appState: AppViewState;
  host: string;
  port: string;
  setHost: (value: string) => void;
  setPort: (value: string) => void;
  connect: () => void;
  disconnect: () => void;
}) {
  return (
    <div className="grid gap-5 lg:grid-cols-[1fr_1fr]">
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <h2 className="text-lg font-semibold">Connection</h2>
        <div className="mt-4 grid gap-3">
          <label className="grid gap-1 text-sm text-slate-300">
            Host
            <input
              className="rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-slate-100"
              value={props.host}
              onChange={(event) => props.setHost(event.target.value)}
              placeholder="Auto-discover"
            />
          </label>
          <label className="grid gap-1 text-sm text-slate-300">
            Port
            <input
              className="rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-slate-100"
              value={props.port}
              onChange={(event) => props.setPort(event.target.value)}
              placeholder="Auto"
              inputMode="numeric"
            />
          </label>
          <div className="flex gap-3">
            <button
              className="rounded-lg bg-cyan-700 px-4 py-2 font-semibold text-white hover:bg-cyan-600"
              onClick={props.connect}
            >
              Connect
            </button>
            <button
              className="rounded-lg border border-slate-700 px-4 py-2 font-semibold text-slate-100 hover:bg-slate-800"
              onClick={props.disconnect}
            >
              Disconnect
            </button>
          </div>
        </div>
      </section>
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <h2 className="text-lg font-semibold">Status</h2>
        <dl className="mt-4 grid gap-2 text-sm">
          <StatusRow label="Connection" value={props.appState.connection} />
          <StatusRow label="Scenes" value={String(props.appState.sceneCount)} />
          <StatusRow label="Channels" value={String(props.appState.channelCount)} />
          <StatusRow label="Last Event" value={props.appState.lastEventAt ?? "None"} />
        </dl>
      </section>
    </div>
  );
}

function SceneTab(props: {
  appState: AppViewState;
  selectScene: (sceneId: string) => void;
  setSceneFadeEnabled: (sceneId: string, enabled: boolean) => void;
  setSceneDurationMs: (sceneId: string, durationMs: number) => void;
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
            <label className="mt-4 flex w-full max-w-xs flex-col gap-1 text-sm text-slate-300">
              Fade duration (seconds)
              <input
                className="rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-slate-100"
                max={120}
                min={0.1}
                onChange={(event) => {
                  const seconds = Number(event.target.value);
                  if (Number.isFinite(seconds)) {
                    props.setSceneDurationMs(selected.sceneId, Math.round(seconds * 1000));
                  }
                }}
                step={0.1}
                type="number"
                value={(selected.durationMs / 1000).toFixed(1)}
              />
            </label>

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

function formatDb(value: number) {
  return `${value.toFixed(1)} dB`;
}

function channelName(channels: ChannelSummary[], group: number, channel: number) {
  return channels.find((entry) => entry.group === group && entry.channel === channel)?.name ?? "Unknown";
}

function LogsTab({ appState }: { appState: AppViewState }) {
  return (
    <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
      <h2 className="text-lg font-semibold">Logs</h2>
      <div className="mt-4 max-h-[34rem] overflow-auto rounded-lg border border-slate-800">
        {appState.logs.length === 0 ? (
          <p className="p-3 text-sm text-slate-400">No events yet.</p>
        ) : (
          appState.logs.map((entry) => (
            <div
              className="grid grid-cols-[9rem_5rem_1fr] gap-3 border-b border-slate-800 px-3 py-2 text-sm last:border-b-0"
              key={entry.id}
            >
              <span className="text-slate-500">{entry.timestamp}</span>
              <span className="uppercase text-slate-400">{entry.source}</span>
              <span>{entry.message}</span>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function StatusRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-4 border-b border-slate-800 py-2 last:border-b-0">
      <dt className="text-slate-500">{label}</dt>
      <dd className="text-right text-slate-100">{value}</dd>
    </div>
  );
}

async function refreshAppState(
  setAppState: (appState: AppViewState) => void,
  setCommandError: (message: string | null) => void,
) {
  try {
    setAppState(await invoke<AppViewState>("get_app_status"));
  } catch (error) {
    setCommandError(String(error));
  }
}

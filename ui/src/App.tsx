import { type ReactNode, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { disconnectedSnapshot, type AppSnapshot } from "./types";

type Tab = "connection" | "scene" | "logs";

export default function App() {
  const [activeTab, setActiveTab] = useState<Tab>("connection");
  const [host, setHost] = useState("");
  const [port, setPort] = useState("");
  const [commandError, setCommandError] = useState<string | null>(null);
  const [snapshot, setSnapshot] = useState<AppSnapshot>(disconnectedSnapshot);

  useEffect(() => {
    let cancelled = false;
    void refreshSnapshot(setSnapshot, setCommandError);

    const unlistenPromise = listen<AppSnapshot>("app-status-changed", (event) => {
      if (cancelled) {
        return;
      }
      setSnapshot(event.payload);
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
      const next = await invoke<AppSnapshot>(command, args);
      setSnapshot(next);
    } catch (error) {
      setCommandError(String(error));
      await refreshSnapshot(setSnapshot, setCommandError);
    }
  }

  async function runVoidCommand(command: string) {
    setCommandError(null);
    try {
      await invoke(command);
      await refreshSnapshot(setSnapshot, setCommandError);
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
              {snapshot.currentScene
                ? `Scene ${snapshot.currentScene.index}: ${snapshot.currentScene.name}`
                : "No LV1 scene selected"}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <StatusBadge
              label={snapshot.connection}
              tone={snapshot.connection === "connected" ? "good" : "neutral"}
            />
            <StatusBadge
              label={`Fade: ${snapshot.fadeState}`}
              tone={snapshot.fadeState === "blocked" ? "warning" : "neutral"}
            />
            <button
              className={
                snapshot.lockout
                  ? "rounded-full border border-amber-500/60 bg-amber-950 px-3 py-1 text-sm text-amber-100"
                  : "rounded-full border border-slate-700 bg-slate-800 px-3 py-1 text-sm text-slate-200"
              }
              onClick={() => runSnapshotCommand("set_lockout", { enabled: !snapshot.lockout })}
            >
              {snapshot.lockout ? "Lockout On" : "Lockout Off"}
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
            snapshot={snapshot}
            setHost={setHost}
            setPort={setPort}
            connect={connect}
            disconnect={() => runSnapshotCommand("disconnect_lv1")}
          />
        )}
        {activeTab === "scene" && <SceneTab snapshot={snapshot} />}
        {activeTab === "logs" && <LogsTab snapshot={snapshot} />}
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

function ConnectionTab(props: {
  snapshot: AppSnapshot;
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
          <StatusRow label="Connection" value={props.snapshot.connection} />
          <StatusRow label="Scenes" value={String(props.snapshot.sceneCount)} />
          <StatusRow label="Channels" value={String(props.snapshot.channelCount)} />
          <StatusRow label="Last Event" value={props.snapshot.lastEventAt ?? "None"} />
        </dl>
      </section>
    </div>
  );
}

function SceneTab({ snapshot }: { snapshot: AppSnapshot }) {
  return (
    <div className="grid gap-5 lg:grid-cols-[1fr_1fr]">
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <h2 className="text-lg font-semibold">Current Scene</h2>
        <p className="mt-2 text-slate-300">
          {snapshot.currentScene ? `${snapshot.currentScene.index}: ${snapshot.currentScene.name}` : "No current scene reported."}
        </p>
        <p className="mt-4 rounded-lg border border-slate-800 bg-slate-950 p-3 text-sm text-slate-400">
          Capture and save workflow will be added in the next phase.
        </p>
      </section>
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <h2 className="text-lg font-semibold">Scene List</h2>
        <div className="mt-4 max-h-96 overflow-auto rounded-lg border border-slate-800">
          {snapshot.scenes.length === 0 ? (
            <p className="p-3 text-sm text-slate-400">No scenes loaded.</p>
          ) : (
            snapshot.scenes.map((scene) => (
              <div className="border-b border-slate-800 px-3 py-2 text-sm last:border-b-0" key={`${scene.index}-${scene.name}`}>
                {scene.index}: {scene.name}
              </div>
            ))
          )}
        </div>
      </section>
    </div>
  );
}

function LogsTab({ snapshot }: { snapshot: AppSnapshot }) {
  return (
    <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
      <h2 className="text-lg font-semibold">Logs</h2>
      <div className="mt-4 max-h-[34rem] overflow-auto rounded-lg border border-slate-800">
        {snapshot.logs.length === 0 ? (
          <p className="p-3 text-sm text-slate-400">No events yet.</p>
        ) : (
          snapshot.logs.map((entry) => (
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

async function refreshSnapshot(
  setSnapshot: (snapshot: AppSnapshot) => void,
  setCommandError: (message: string | null) => void,
) {
  try {
    setSnapshot(await invoke<AppSnapshot>("get_app_status"));
  } catch (error) {
    setCommandError(String(error));
  }
}

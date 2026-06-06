import { useState } from "react";
import type { AppSnapshot } from "./types";

const initialSnapshot: AppSnapshot = {
  connection: "disconnected",
  currentScene: null,
  scenes: [],
  sceneCount: 0,
  channelCount: 0,
  fadeState: "idle",
  lockout: false,
  logs: [],
  lastEventAt: null,
};

type Tab = "connection" | "scene" | "logs";

export default function App() {
  const [activeTab, setActiveTab] = useState<Tab>("connection");
  const snapshot = initialSnapshot;

  return (
    <main className="min-h-screen bg-slate-950 text-slate-100">
      <header className="border-b border-slate-800 bg-slate-900/80 px-6 py-4">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold">LV1 Scene Fade Utility</h1>
            <p className="text-sm text-slate-400">Desktop shell</p>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <StatusBadge label={snapshot.connection} tone="neutral" />
            <StatusBadge label={`Fade: ${snapshot.fadeState}`} tone="neutral" />
            <StatusBadge label={snapshot.lockout ? "Lockout On" : "Lockout Off"} tone={snapshot.lockout ? "warning" : "neutral"} />
            <button className="rounded-lg bg-red-700 px-5 py-3 font-bold text-white shadow-lg shadow-red-950/40 hover:bg-red-600">
              Abort All
            </button>
          </div>
        </div>
      </header>

      <nav className="border-b border-slate-800 px-6">
        <div className="flex gap-2">
          <TabButton active={activeTab === "connection"} onClick={() => setActiveTab("connection")}>Connection</TabButton>
          <TabButton active={activeTab === "scene"} onClick={() => setActiveTab("scene")}>Scene</TabButton>
          <TabButton active={activeTab === "logs"} onClick={() => setActiveTab("logs")}>Logs</TabButton>
        </div>
      </nav>

      <section className="p-6">
        {activeTab === "connection" && <ConnectionTab snapshot={snapshot} />}
        {activeTab === "scene" && <SceneTab snapshot={snapshot} />}
        {activeTab === "logs" && <LogsTab snapshot={snapshot} />}
      </section>
    </main>
  );
}

function TabButton(props: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      className={props.active ? "border-b-2 border-cyan-400 px-4 py-3 text-cyan-200" : "px-4 py-3 text-slate-400 hover:text-slate-100"}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}

function StatusBadge(props: { label: string; tone: "neutral" | "warning" }) {
  const tone = props.tone === "warning" ? "border-amber-500/60 bg-amber-950 text-amber-100" : "border-slate-700 bg-slate-800 text-slate-200";
  return <span className={`rounded-full border px-3 py-1 text-sm ${tone}`}>{props.label}</span>;
}

function ConnectionTab({ snapshot }: { snapshot: AppSnapshot }) {
  return (
    <div className="rounded-xl border border-slate-800 bg-slate-900 p-5">
      <h2 className="text-lg font-semibold">Connection</h2>
      <p className="mt-2 text-slate-400">Status: {snapshot.connection}</p>
    </div>
  );
}

function SceneTab({ snapshot }: { snapshot: AppSnapshot }) {
  return (
    <div className="rounded-xl border border-slate-800 bg-slate-900 p-5">
      <h2 className="text-lg font-semibold">Scene</h2>
      <p className="mt-2 text-slate-400">Current scene: {snapshot.currentScene ? `${snapshot.currentScene.index}: ${snapshot.currentScene.name}` : "None"}</p>
      <p className="mt-1 text-slate-400">Known channels: {snapshot.channelCount}</p>
    </div>
  );
}

function LogsTab({ snapshot }: { snapshot: AppSnapshot }) {
  return (
    <div className="rounded-xl border border-slate-800 bg-slate-900 p-5">
      <h2 className="text-lg font-semibold">Logs</h2>
      <p className="mt-2 text-slate-400">{snapshot.logs.length === 0 ? "No events yet." : `${snapshot.logs.length} events`}</p>
    </div>
  );
}

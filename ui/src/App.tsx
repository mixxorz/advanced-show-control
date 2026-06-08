import { type ReactNode, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { disconnectedAppViewState, type AppViewState } from "./types";
import { refreshAppState, runSnapshotCommand, runVoidCommand } from "./commands";
import { ConnectionTab } from "./components/ConnectionTab";
import { Header } from "./components/Header";
import { LogsTab } from "./components/LogsTab";
import { SceneTab } from "./components/SceneTab";

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

  async function connect() {
    const args: { host?: string; port?: number } = {};

    if (host.trim()) {
      args.host = host.trim();
    }

    const parsedPort = Number(port);
    if (Number.isInteger(parsedPort) && parsedPort > 0) {
      args.port = parsedPort;
    }

    await runSnapshotCommand("connect_lv1", args, setAppState, setCommandError);
  }

  return (
    <main className="min-h-screen bg-slate-950 text-slate-100">
      <Header
        appState={appState}
        commandError={commandError}
        onAbortAll={() => runVoidCommand("abort_all_fades", setAppState, setCommandError)}
        onNewShowFile={() => runSnapshotCommand("new_show_file", undefined, setAppState, setCommandError)}
        onOpenShowFile={() => runSnapshotCommand("open_show_file_dialog", undefined, setAppState, setCommandError)}
        onSaveShowFile={() => runSnapshotCommand("save_show_file", undefined, setAppState, setCommandError)}
        onSaveShowFileAs={() => runSnapshotCommand("save_show_file_as_dialog", undefined, setAppState, setCommandError)}
        onToggleLockout={() => runSnapshotCommand("set_lockout", { enabled: !appState.lockout }, setAppState, setCommandError)}
      />

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
          <ConnectionTab appState={appState} connect={connect} disconnect={() => runSnapshotCommand("disconnect_lv1", undefined, setAppState, setCommandError)} host={host} port={port} setHost={setHost} setPort={setPort} />
        )}
        {activeTab === "scene" && (
          <SceneTab
            appState={appState}
            selectScene={(sceneId: string) => runSnapshotCommand("select_scene_config", { sceneId }, setAppState, setCommandError)}
            setSceneDurationMs={(sceneId: string, durationMs: number) =>
              runSnapshotCommand("set_scene_duration_ms", { sceneId, durationMs }, setAppState, setCommandError)
            }
            storeSceneConfig={(sceneId: string) =>
              runSnapshotCommand("store_scene_config", { sceneId }, setAppState, setCommandError)
            }
            setAllChannelsScoped={(sceneId: string, scoped: boolean) =>
              runSnapshotCommand("set_all_channels_scoped", { sceneId, scoped }, setAppState, setCommandError)
            }
            setChannelScoped={(sceneId: string, group: number, channel: number, scoped: boolean) =>
              runSnapshotCommand("set_channel_scoped", { sceneId, group, channel, scoped }, setAppState, setCommandError)
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

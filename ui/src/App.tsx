import { type ReactNode, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { disconnectedAppViewState, type AppViewState } from "./types";
import {
  connectLv1System,
  reconnectTimedOut,
  refreshLv1Discovery,
  runSnapshotCommand,
  runVoidCommand,
  startupAutoConnectLv1,
} from "./commands";
import { ConnectionScreen } from "./components/ConnectionScreen";
import { Header } from "./components/Header";
import { LogsTab } from "./components/LogsTab";
import { SceneTab } from "./components/SceneTab";

type MainTab = "scene" | "logs";

export default function App() {
  const [activeTab, setActiveTab] = useState<MainTab>("scene");
  const [showConnection, setShowConnection] = useState(true);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [appState, setAppState] = useState<AppViewState>(disconnectedAppViewState);

  useEffect(() => {
    let cancelled = false;
    void startupAutoConnectLv1()
      .then((snapshot) => {
        if (cancelled) {
          return;
        }
        setAppState(snapshot);
        setShowConnection(snapshot.connection !== "connected");
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setCommandError(String(error));
        setShowConnection(true);
      });

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

  useEffect(() => {
    if (!showConnection) {
      return;
    }

    let cancelled = false;

    async function refreshDiscovery() {
      try {
        const snapshot = await refreshLv1Discovery();
        if (cancelled) {
          return;
        }
        setCommandError(null);
        setAppState(snapshot);
      } catch (error) {
        if (!cancelled) {
          setCommandError(String(error));
        }
      }
    }

    void refreshDiscovery();
    const interval = window.setInterval(() => {
      void refreshDiscovery();
    }, 5000);

    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [showConnection]);

  useEffect(() => {
    if (!appState.reconnect.active) {
      return;
    }

    const attempt = appState.reconnect.attempt;
    const timer = window.setTimeout(async () => {
      try {
        const snapshot = await reconnectTimedOut(attempt);
        setAppState(snapshot);
        if (!snapshot.reconnect.active && snapshot.connection !== "connected") {
          setShowConnection(true);
        }
      } catch (error) {
        setCommandError(String(error));
        setShowConnection(true);
      }
    }, 15000);

    return () => window.clearTimeout(timer);
  }, [appState.reconnect.active, appState.reconnect.attempt]);

  if (showConnection) {
    return (
      <ConnectionScreen
        appState={appState}
        commandError={commandError}
        onResume={() => setShowConnection(false)}
        onSelectSystem={async (identity) => {
          setCommandError(null);
          try {
            const snapshot = await connectLv1System(identity);
            setAppState(snapshot);
            if (snapshot.connection === "connected") {
              setShowConnection(false);
            }
          } catch (error) {
            setCommandError(String(error));
          }
        }}
      />
    );
  }

  return (
    <main className="min-h-screen bg-slate-950 text-slate-100">
      <Header
        appState={appState}
        commandError={commandError}
        onAbortAll={() => runVoidCommand("abort_all_fades", setAppState, setCommandError)}
        onFinishNow={() => runVoidCommand("finish_fade_now", setAppState, setCommandError)}
        onNewShowFile={() => runSnapshotCommand("new_show_file", undefined, setAppState, setCommandError)}
        onOpenConnection={() => setShowConnection(true)}
        onOpenShowFile={() => runSnapshotCommand("open_show_file_dialog", undefined, setAppState, setCommandError)}
        onSaveShowFile={() => runSnapshotCommand("save_show_file", undefined, setAppState, setCommandError)}
        onSaveShowFileAs={() => runSnapshotCommand("save_show_file_as_dialog", undefined, setAppState, setCommandError)}
        onToggleLockout={() => runSnapshotCommand("set_lockout", { enabled: !appState.lockout }, setAppState, setCommandError)}
      />

      <nav className="border-b border-slate-800 px-6">
        <div className="flex gap-2">
          <TabButton active={activeTab === "scene"} onClick={() => setActiveTab("scene")}>
            Scene
          </TabButton>
          <TabButton active={activeTab === "logs"} onClick={() => setActiveTab("logs")}>
            Logs
          </TabButton>
        </div>
      </nav>

      <section className="p-6">
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

      {appState.reconnect.active && (
        <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/70">
          <div className="rounded-xl border border-slate-700 bg-slate-900 px-8 py-6 text-xl font-semibold text-slate-100 shadow-2xl">
            Reconnecting...
          </div>
        </div>
      )}
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

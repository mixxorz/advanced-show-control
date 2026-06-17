import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { disconnectedAppViewState, type AppViewState } from "./types";
import {
  attemptReconnectLv1,
  connectLv1System,
  reconnectTimedOut,
  refreshLv1Discovery,
  runSnapshotCommand,
  runVoidCommand,
  startupAutoConnectLv1,
  setSceneScopePanEnabled,
} from "./commands";
import { AppShell, type MainTab } from "./components/AppShell";

export default function App() {
  const [activeTab, setActiveTab] = useState<MainTab>("scene");
  const [showConnection, setShowConnection] = useState(true);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [appState, setAppState] = useState<AppViewState>(disconnectedAppViewState);

  const applySnapshot = useCallback((next: AppViewState) => {
    setAppState(prev =>
      !prev || next.stateVersion > prev.stateVersion ? next : prev
    );
  }, []);

  useEffect(() => {
    let cancelled = false;
    void startupAutoConnectLv1()
      .then((snapshot) => {
        if (cancelled) {
          return;
        }
        applySnapshot(snapshot);
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
      applySnapshot(event.payload);
    });

    return () => {
      cancelled = true;
      void unlistenPromise.then((unlisten) => {
        void unlisten();
      });
    };
  }, [applySnapshot]);

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
        applySnapshot(snapshot);
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
  }, [showConnection, applySnapshot]);

  useEffect(() => {
    if (!appState.reconnect.active) {
      return;
    }

    const attempt = appState.reconnect.attempt;
    let cancelled = false;
    let reconnectInFlight = false;

    async function attemptReconnect() {
      if (reconnectInFlight) {
        return;
      }
      reconnectInFlight = true;
      try {
        const snapshot = await attemptReconnectLv1();
        if (cancelled) {
          return;
        }
        applySnapshot(snapshot);
        if (snapshot.connection === "connected") {
          setCommandError(null);
          setShowConnection(false);
        }
      } catch (error) {
        if (!cancelled) {
          setCommandError(String(error));
        }
      } finally {
        reconnectInFlight = false;
      }
    }

    void attemptReconnect();
    const interval = window.setInterval(() => {
      void attemptReconnect();
    }, 2000);

    const timer = window.setTimeout(async () => {
      try {
        const snapshot = await reconnectTimedOut(attempt);
        if (cancelled) {
          return;
        }
        applySnapshot(snapshot);
        if (!snapshot.reconnect.active && snapshot.connection !== "connected") {
          setShowConnection(true);
        }
      } catch (error) {
        if (!cancelled) {
          setCommandError(String(error));
          setShowConnection(true);
        }
      }
    }, 15000);

    return () => {
      cancelled = true;
      window.clearInterval(interval);
      window.clearTimeout(timer);
    };
  }, [appState.reconnect.active, appState.reconnect.attempt, applySnapshot]);

  return (
    <AppShell
      activeTab={activeTab}
      appState={appState}
      commandError={commandError}
      onAbortAll={() => runVoidCommand("abort_all_fades", applySnapshot, setCommandError)}
      onDisconnect={async () => {
        await runSnapshotCommand("disconnect_lv1", undefined, applySnapshot, setCommandError);
        setShowConnection(true);
      }}
      onNewShowFile={() => runSnapshotCommand("new_show_file", undefined, applySnapshot, setCommandError)}
      onOpenConnection={() => setShowConnection(true)}
      onOpenShowFile={() => runSnapshotCommand("open_show_file_dialog", undefined, applySnapshot, setCommandError)}
      onResume={() => setShowConnection(false)}
      onSaveShowFile={() => runSnapshotCommand("save_show_file", undefined, applySnapshot, setCommandError)}
      onSaveShowFileAs={() => runSnapshotCommand("save_show_file_as_dialog", undefined, applySnapshot, setCommandError)}
      onSelectScene={(sceneId: string) => runSnapshotCommand("select_scene_config", { sceneId }, applySnapshot, setCommandError)}
      onSelectSystem={async (identity) => {
        setCommandError(null);
        try {
          const snapshot = await connectLv1System(identity);
          applySnapshot(snapshot);
          if (snapshot.connection === "connected") {
            setShowConnection(false);
          }
        } catch (error) {
          setCommandError(String(error));
        }
      }}
      onSelectTab={setActiveTab}
      onSetAllChannelsScoped={(sceneId: string, scoped: boolean) =>
        runSnapshotCommand("set_all_channels_scoped", { sceneId, scoped }, applySnapshot, setCommandError)
      }
      onSetChannelScoped={(sceneId: string, group: number, channel: number, scoped: boolean) =>
        runSnapshotCommand("set_channel_scoped", { sceneId, group, channel, scoped }, applySnapshot, setCommandError)
      }
      onSetSceneDurationMs={(sceneId: string, durationMs: number) =>
        runSnapshotCommand("set_scene_duration_ms", { sceneId, durationMs }, applySnapshot, setCommandError)
      }
      onSetSceneScopeFadersEnabled={(sceneId: string, enabled: boolean) =>
        runSnapshotCommand("set_scene_scope_faders_enabled", { sceneId, enabled }, applySnapshot, setCommandError)
      }
      onSetSceneScopePanEnabled={(sceneId: string, enabled: boolean) =>
        setSceneScopePanEnabled(sceneId, enabled, applySnapshot, setCommandError)
      }
      onStoreSceneConfig={(sceneId: string) =>
        runSnapshotCommand("store_scene_config", { sceneId }, applySnapshot, setCommandError)
      }
      onToggleLockout={() => runSnapshotCommand("set_lockout", { enabled: !appState.lockout }, applySnapshot, setCommandError)}
      showConnection={showConnection}
    />
  );
}

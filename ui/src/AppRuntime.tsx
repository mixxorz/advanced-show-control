import { useCallback, useEffect, useState } from "react";
import { useRef } from "react";
import {
  AppCommandsProvider,
  AppStateProvider,
  type AppCommands,
} from "./appContext";
import { AppShell, type MainTab } from "./components/AppShell";
import {
  disconnectedAppViewState,
  type AppViewState,
  type Lv1SystemIdentity,
} from "./types";

export type AppStatusListener = (appState: AppViewState) => void;

export type AppRuntimeServices = {
  abortAll: () => Promise<void> | void;
  attemptReconnectLv1: () => Promise<AppViewState>;
  connectLv1System: (identity: Lv1SystemIdentity) => Promise<AppViewState>;
  disconnectLv1: () => Promise<AppViewState>;
  listenForAppStatus: (listener: AppStatusListener) => Promise<() => void>;
  newShowFile: () => Promise<AppViewState>;
  openShowFile: () => Promise<AppViewState>;
  reconnectTimedOut: (attempt: number) => Promise<AppViewState>;
  refreshAppState: () => Promise<AppViewState>;
  refreshLv1Discovery: () => Promise<AppViewState>;
  saveShowFile: () => Promise<AppViewState>;
  saveShowFileAs: () => Promise<AppViewState>;
  selectSceneConfig: (sceneId: string) => Promise<AppViewState>;
  setAllChannelsScoped: (
    sceneId: string,
    scoped: boolean,
  ) => Promise<AppViewState>;
  setChannelScoped: (
    sceneId: string,
    group: number,
    channel: number,
    scoped: boolean,
  ) => Promise<AppViewState>;
  setLockout: (enabled: boolean) => Promise<AppViewState>;
  setSceneDurationMs: (
    sceneId: string,
    durationMs: number,
  ) => Promise<AppViewState>;
  setSceneScopeFadersEnabled: (
    sceneId: string,
    enabled: boolean,
  ) => Promise<AppViewState>;
  setSceneScopePanEnabled: (
    sceneId: string,
    enabled: boolean,
  ) => Promise<AppViewState>;
  startupAutoConnectLv1: () => Promise<AppViewState>;
  storeSceneConfig: (sceneId: string) => Promise<AppViewState>;
};

type ConnectionModalMode = "startup" | "manual" | null;

export function AppRuntime(props: { services: AppRuntimeServices }) {
  const { services } = props;
  const [activeTab, setActiveTab] = useState<MainTab>("scenes");
  const [connectionModalMode, setConnectionModalMode] =
    useState<ConnectionModalMode>("startup");
  const [commandError, setCommandError] = useState<string | null>(null);
  const [appState, setAppState] = useState<AppViewState>(
    disconnectedAppViewState,
  );
  const latestAppState = useRef(disconnectedAppViewState);
  const hasAppliedSnapshot = useRef(false);
  const showConnection = connectionModalMode !== null;

  const applySnapshot = useCallback((next: AppViewState) => {
    const accepted =
      !hasAppliedSnapshot.current ||
      next.stateVersion > latestAppState.current.stateVersion;
    if (accepted) {
      latestAppState.current = next;
      setAppState(next);
    }
    hasAppliedSnapshot.current = true;
    return accepted;
  }, []);

  const closeStartupModalIfConnected = useCallback((snapshot: AppViewState) => {
    if (snapshot.connection === "connected") {
      setConnectionModalMode((mode) => (mode === "startup" ? null : mode));
    }
  }, []);

  const runSnapshot = useCallback(
    async (command: () => Promise<AppViewState>) => {
      setCommandError(null);
      try {
        const snapshot = await command();
        applySnapshot(snapshot);
        return true;
      } catch (error) {
        setCommandError(String(error));
        try {
          applySnapshot(await services.refreshAppState());
        } catch (refreshError) {
          setCommandError(String(refreshError));
        }
        return false;
      }
    },
    [applySnapshot, services],
  );

  useEffect(() => {
    let cancelled = false;
    void services
      .startupAutoConnectLv1()
      .then((snapshot) => {
        if (cancelled) return;
        const accepted = applySnapshot(snapshot);
        if (accepted) {
          closeStartupModalIfConnected(snapshot);
        }
      })
      .catch((error) => {
        if (cancelled) return;
        setCommandError(String(error));
        setConnectionModalMode("startup");
      });

    const unlistenPromise = services.listenForAppStatus((snapshot) => {
      if (!cancelled && applySnapshot(snapshot)) {
        closeStartupModalIfConnected(snapshot);
      }
    });

    return () => {
      cancelled = true;
      void unlistenPromise.then((unlisten) => {
        void unlisten();
      });
    };
  }, [applySnapshot, closeStartupModalIfConnected, services]);

  useEffect(() => {
    if (!showConnection) return;
    let cancelled = false;

    async function refreshDiscovery() {
      try {
        const snapshot = await services.refreshLv1Discovery();
        if (cancelled) return;
        setCommandError(null);
        applySnapshot(snapshot);
      } catch (error) {
        if (!cancelled) setCommandError(String(error));
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
  }, [showConnection, applySnapshot, services]);

  useEffect(() => {
    if (!appState.reconnect.active) return;
    const attempt = appState.reconnect.attempt;
    let cancelled = false;
    let reconnectInFlight = false;

    async function attemptReconnect() {
      if (reconnectInFlight) return;
      reconnectInFlight = true;
      try {
        const snapshot = await services.attemptReconnectLv1();
        if (cancelled) return;
        applySnapshot(snapshot);
        if (snapshot.connection === "connected") {
          setCommandError(null);
          setConnectionModalMode(null);
        }
      } catch (error) {
        if (!cancelled) setCommandError(String(error));
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
        const snapshot = await services.reconnectTimedOut(attempt);
        if (cancelled) return;
        applySnapshot(snapshot);
        if (!snapshot.reconnect.active && snapshot.connection !== "connected") {
          setConnectionModalMode("startup");
        }
      } catch (error) {
        if (!cancelled) {
          setCommandError(String(error));
          setConnectionModalMode("startup");
        }
      }
    }, 15000);

    return () => {
      cancelled = true;
      window.clearInterval(interval);
      window.clearTimeout(timer);
    };
  }, [
    appState.reconnect.active,
    appState.reconnect.attempt,
    applySnapshot,
    services,
  ]);

  const commands: AppCommands = {
    abortAll: () => {
      setCommandError(null);
      void Promise.resolve(services.abortAll()).catch((error) => {
        setCommandError(String(error));
      });
    },
    disconnect: async () => {
      await runSnapshot(() => services.disconnectLv1());
      setConnectionModalMode("manual");
    },
    newShowFile: () => runSnapshot(() => services.newShowFile()),
    openShowFile: () => runSnapshot(() => services.openShowFile()),
    saveShowFile: () => runSnapshot(() => services.saveShowFile()),
    saveShowFileAs: () => runSnapshot(() => services.saveShowFileAs()),
    selectScene: (sceneId: string) =>
      runSnapshot(() => services.selectSceneConfig(sceneId)),
    selectSystem: async (identity) => {
      setCommandError(null);
      try {
        const snapshot = await services.connectLv1System(identity);
        applySnapshot(snapshot);
        if (
          snapshot.connection === "connected" &&
          identityMatches(snapshot.connectedLv1Identity, identity)
        ) {
          setConnectionModalMode(null);
        }
      } catch (error) {
        setCommandError(String(error));
      }
    },
    setAllChannelsScoped: (sceneId: string, scoped: boolean) =>
      runSnapshot(() => services.setAllChannelsScoped(sceneId, scoped)),
    setChannelScoped: (sceneId, group, channel, scoped) =>
      runSnapshot(() =>
        services.setChannelScoped(sceneId, group, channel, scoped),
      ),
    setSceneDurationMs: (sceneId, durationMs) =>
      runSnapshot(() => services.setSceneDurationMs(sceneId, durationMs)),
    setSceneScopeFadersEnabled: (sceneId, enabled) =>
      runSnapshot(() => services.setSceneScopeFadersEnabled(sceneId, enabled)),
    setSceneScopePanEnabled: (sceneId, enabled) =>
      runSnapshot(() => services.setSceneScopePanEnabled(sceneId, enabled)),
    storeSceneConfig: (sceneId) =>
      runSnapshot(() => services.storeSceneConfig(sceneId)),
    toggleLockout: () =>
      runSnapshot(() => services.setLockout(!appState.lockout)),
  };

  return (
    <AppStateProvider appState={appState} commandError={commandError}>
      <AppCommandsProvider commands={commands}>
        <AppShell
          activeTab={activeTab}
          onOpenConnection={() => setConnectionModalMode("manual")}
          onResume={() => setConnectionModalMode(null)}
          onSelectTab={setActiveTab}
          showConnection={showConnection}
        />
      </AppCommandsProvider>
    </AppStateProvider>
  );
}

function identityMatches(
  connected: Lv1SystemIdentity | null,
  selected: Lv1SystemIdentity,
) {
  if (!connected) return false;
  if (connected.uuid && selected.uuid) return connected.uuid === selected.uuid;
  return (
    connected.host === selected.host &&
    connected.address === selected.address &&
    connected.port === selected.port
  );
}

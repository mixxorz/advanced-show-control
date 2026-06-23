import { useCallback, useEffect, useState } from "react";
import { useRef } from "react";
import {
  AppCommandsProvider,
  AppStateProvider,
  type AppCommands,
} from "./appContext";
import { AppShell, type MainTab } from "./components/AppShell";
import { formatSessionWindowTitle } from "./sessionTitle";
import {
  disconnectedAppViewState,
  type AppViewState,
  type Lv1SystemIdentity,
} from "./types";

export type AppStatusListener = (appState: AppViewState) => void;

export type AppRuntimeServices = {
  frontendReady: () => Promise<void>;
  abortAll: () => Promise<void> | void;
  attemptReconnectLv1: () => Promise<unknown>;
  connectLv1System: (identity: Lv1SystemIdentity) => Promise<unknown>;
  disconnectLv1: () => Promise<unknown>;
  listenForAppStatus: (listener: AppStatusListener) => Promise<() => void>;
  newShowFile: () => Promise<unknown>;
  openShowFile: () => Promise<unknown>;
  reconnectTimedOut: (attempt: number) => Promise<unknown>;
  refreshLv1Discovery: () => Promise<unknown>;
  saveShowFile: () => Promise<unknown>;
  saveShowFileAs: () => Promise<unknown>;
  cueScene: (internalSceneId: string) => Promise<unknown>;
  recallScene: (internalSceneId: string) => Promise<unknown>;
  selectSceneConfig: (internalSceneId: string) => Promise<unknown>;
  setAllChannelsScoped: (
    internalSceneId: string,
    scoped: boolean,
  ) => Promise<unknown>;
  setChannelScoped: (
    internalSceneId: string,
    group: number,
    channel: number,
    scoped: boolean,
  ) => Promise<unknown>;
  setLockout: (enabled: boolean) => Promise<unknown>;
  setSceneDurationMs: (
    internalSceneId: string,
    durationMs: number,
  ) => Promise<unknown>;
  setSceneScopeFadersEnabled: (
    internalSceneId: string,
    enabled: boolean,
  ) => Promise<unknown>;
  setSceneScopePanEnabled: (
    internalSceneId: string,
    enabled: boolean,
  ) => Promise<unknown>;
  setWindowTitle?: (title: string) => Promise<unknown> | void;
  startupAutoConnectLv1: () => Promise<unknown>;
  storeSceneConfig: (internalSceneId: string) => Promise<unknown>;
  linkSceneConfig: (
    sourceInternalSceneId: string,
    targetSceneIndex: number,
    overwriteExisting: boolean,
  ) => Promise<unknown>;
  deleteSceneConfig: (internalSceneId: string) => Promise<unknown>;
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

  // Async service calls and status events can resolve out of order. Only newer
  // snapshots are allowed to replace the UI projection.
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

  // Startup owns the initial modal, but a manually opened modal should stay open
  // even when the app is already connected.
  const closeStartupModalIfConnected = useCallback((snapshot: AppViewState) => {
    if (snapshot.connection === "connected") {
      setConnectionModalMode((mode) => (mode === "startup" ? null : mode));
    }
  }, []);

  const runCommand = useCallback(async (command: () => Promise<unknown>) => {
    setCommandError(null);
    try {
      await command();
      return true;
    } catch (error) {
      setCommandError(String(error));
      return false;
    }
  }, []);

  // Kick off startup auto-connect while also subscribing to backend status
  // updates. Either path may provide the first fresh connected snapshot.
  useEffect(() => {
    let cancelled = false;
    let unlisten: null | (() => void) = null;

    async function start() {
      try {
        unlisten = await services.listenForAppStatus((snapshot) => {
          if (!cancelled && applySnapshot(snapshot)) {
            closeStartupModalIfConnected(snapshot);
          }
        });
        if (cancelled) {
          unlisten();
          return;
        }
        await services.frontendReady();
        await services.startupAutoConnectLv1();
      } catch (error) {
        if (!cancelled) {
          setCommandError(String(error));
          setConnectionModalMode("startup");
        }
      }
    }

    void start();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [applySnapshot, closeStartupModalIfConnected, services]);

  // The connection modal doubles as discovery UI, so discovery polling is scoped
  // to the time the modal is visible.
  useEffect(() => {
    if (!showConnection) return;
    let cancelled = false;

    async function refreshDiscovery() {
      try {
        await services.refreshLv1Discovery();
        if (cancelled) return;
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
  }, [showConnection, services]);

  // During backend-managed reconnect, keep retrying briefly before handing the
  // engineer back to the connection modal.
  useEffect(() => {
    if (!appState.reconnect.active) return;
    const attempt = appState.reconnect.attempt;
    let cancelled = false;
    let reconnectInFlight = false;

    async function attemptReconnect() {
      if (reconnectInFlight) return;
      reconnectInFlight = true;
      try {
        await services.attemptReconnectLv1();
        if (cancelled) return;
        setCommandError(null);
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
        await services.reconnectTimedOut(attempt);
        if (cancelled) return;
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
  }, [appState.reconnect.active, appState.reconnect.attempt, services]);

  const commands: AppCommands = {
    abortAll: () => {
      setCommandError(null);
      void Promise.resolve(services.abortAll()).catch((error) => {
        setCommandError(String(error));
      });
    },
    disconnect: async () => {
      await runCommand(() => services.disconnectLv1());
      // Disconnect is an explicit connection-management action; keep the modal
      // open so the engineer can immediately choose another console.
      setConnectionModalMode("manual");
    },
    newShowFile: () => runCommand(() => services.newShowFile()),
    openShowFile: () => runCommand(() => services.openShowFile()),
    cueScene: (internalSceneId) =>
      runCommand(() => services.cueScene(internalSceneId)),
    recallScene: (internalSceneId) =>
      runCommand(() => services.recallScene(internalSceneId)),
    saveShowFile: () => runCommand(() => services.saveShowFile()),
    saveShowFileAs: () => runCommand(() => services.saveShowFileAs()),
    selectScene: (internalSceneId: string) =>
      runCommand(() => services.selectSceneConfig(internalSceneId)),
    selectSystem: async (identity) => {
      setCommandError(null);
      try {
        await services.connectLv1System(identity);
      } catch (error) {
        setCommandError(String(error));
      }
    },
    setAllChannelsScoped: (internalSceneId: string, scoped: boolean) =>
      runCommand(() => services.setAllChannelsScoped(internalSceneId, scoped)),
    setChannelScoped: (internalSceneId, group, channel, scoped) =>
      runCommand(() =>
        services.setChannelScoped(internalSceneId, group, channel, scoped),
      ),
    setSceneDurationMs: (internalSceneId, durationMs) =>
      runCommand(() =>
        services.setSceneDurationMs(internalSceneId, durationMs),
      ),
    setSceneScopeFadersEnabled: (internalSceneId, enabled) =>
      runCommand(() =>
        services.setSceneScopeFadersEnabled(internalSceneId, enabled),
      ),
    setSceneScopePanEnabled: (internalSceneId, enabled) =>
      runCommand(() =>
        services.setSceneScopePanEnabled(internalSceneId, enabled),
      ),
    storeSceneConfig: (internalSceneId) =>
      runCommand(() => services.storeSceneConfig(internalSceneId)),
    toggleLockout: () =>
      runCommand(() => services.setLockout(!appState.lockout)),
  };

  useEffect(() => {
    const title = formatSessionWindowTitle(
      appState.showFileName,
      appState.showFileDirty,
    );
    void Promise.resolve(services.setWindowTitle?.(title)).catch((error) => {
      setCommandError(String(error));
    });
  }, [appState.showFileDirty, appState.showFileName, services]);

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

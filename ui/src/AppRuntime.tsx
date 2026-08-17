import { useCallback, useEffect, useRef, useState } from "react";
import {
  AppCommandsProvider,
  AppStateProvider,
  type AppCommands,
} from "./appContext";
import { AppShell, type MainTab } from "./components/AppShell";
import { formatSessionWindowTitle } from "./sessionTitle";
import {
  isActionShortcutBlocked,
  KeyboardProvider,
  shortcutMatchesEvent,
  useKeyboardHandler,
} from "./keyboard";
import {
  disconnectedAppViewState,
  type AppViewState,
  type Lv1SystemIdentity,
  type TcpConnectLatencyResult,
} from "./types";

export type AppStatusListener = (appState: AppViewState) => void;

export type AppRuntimeServices = {
  frontendReady: () => Promise<void>;
  abortAll: () => Promise<void>;
  connectLv1System: (identity: Lv1SystemIdentity) => Promise<void>;
  copySceneSettings: (internalSceneId: string) => Promise<void>;
  disconnectLv1: () => Promise<void>;
  addSceneToActiveCueList: (
    sceneInternalId: string,
    insertIndex: number,
  ) => Promise<void>;
  createCueList: (name: string) => Promise<void>;
  cueEntry: (cueEntryId: string | null) => Promise<void>;
  deleteCueList: (cueListId: string) => Promise<void>;
  listenForAppStatus: (listener: AppStatusListener) => Promise<() => void>;
  newShowFile: () => Promise<void>;
  openShowFile: () => Promise<void>;
  pasteSceneSettings: (internalSceneId: string) => Promise<void>;
  removeCueEntry: (cueEntryId: string) => Promise<void>;
  recallCuedCue: () => Promise<void>;
  renameCueList: (cueListId: string, name: string) => Promise<void>;
  reorderCueEntries: (orderedEntryIds: string[]) => Promise<void>;
  reorderCueLists: (orderedIds: string[]) => Promise<void>;
  refreshLv1Discovery: () => Promise<void>;
  saveShowFile: () => Promise<void>;
  saveShowFileAs: () => Promise<void>;
  recallScene: (internalSceneId: string) => Promise<void>;
  probeLv1TcpConnectLatency: (
    identity: Lv1SystemIdentity,
    timeoutMs?: number,
  ) => Promise<TcpConnectLatencyResult>;
  selectSceneConfig: (internalSceneId: string) => Promise<void>;
  setAllChannelsScoped: (
    internalSceneId: string,
    scoped: boolean,
  ) => Promise<void>;
  setActiveCueList: (cueListId: string | null) => Promise<void>;
  setChannelScoped: (
    internalSceneId: string,
    group: number,
    channel: number,
    scoped: boolean,
  ) => Promise<void>;
  setLockout: (enabled: boolean) => Promise<void>;
  setSceneDurationMs: (
    internalSceneId: string,
    durationMs: number,
  ) => Promise<void>;
  setSceneScopeFadersEnabled: (
    internalSceneId: string,
    enabled: boolean,
  ) => Promise<void>;
  setSceneScopePanEnabled: (
    internalSceneId: string,
    enabled: boolean,
  ) => Promise<void>;
  setWindowTitle: (title: string) => Promise<void>;
  startupAutoConnectLv1: () => Promise<void>;
  storeSceneConfig: (internalSceneId: string) => Promise<void>;
  linkSceneConfig: (
    sourceInternalSceneId: string,
    targetSceneIndex: number,
    overwriteExisting: boolean,
  ) => Promise<void>;
  deleteSceneConfig: (internalSceneId: string) => Promise<void>;
};

type ConnectionModalMode = "startup" | "manual" | null;

const GO_SHORTCUT_PRIORITY = 100;

function AppShortcutHandler(props: {
  appState: AppViewState;
  commands: AppCommands;
}) {
  const goRecallInFlight = useRef(false);

  useKeyboardHandler({
    id: "app-go-shortcut",
    priority: GO_SHORTCUT_PRIORITY,
    handleKeyDown: (event) => {
      if (isActionShortcutBlocked(event)) {
        return "ignored";
      }
      if (
        !shortcutMatchesEvent(
          props.appState.settings.keyboardShortcuts.go,
          event,
        )
      ) {
        return "ignored";
      }
      if (event.repeat) {
        return "handled";
      }

      const activeCueList = props.appState.cueLists.find(
        (cueList) => cueList.id === props.appState.activeCueListId,
      );
      const cuedEntry = activeCueList?.entries.find(
        (entry) => entry.id === props.appState.cuedCueEntryId,
      );
      const cueIsValid =
        cuedEntry !== undefined &&
        props.appState.sceneConfigs.some(
          (scene) => scene.internalSceneId === cuedEntry.sceneInternalId,
        );

      if (cueIsValid && !goRecallInFlight.current) {
        goRecallInFlight.current = true;
        void Promise.resolve(props.commands.recallCuedCue()).finally(() => {
          goRecallInFlight.current = false;
        });
      }
      return "handled";
    },
  });

  return null;
}

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

  const runCheckedCommand = useCallback(
    async (command: () => Promise<void>) => {
      setCommandError(null);
      try {
        await command();
        return true;
      } catch (error) {
        setCommandError(String(error));
        return false;
      }
    },
    [],
  );

  const runCommand = useCallback(
    async (command: () => Promise<void>) => {
      await runCheckedCommand(command);
    },
    [runCheckedCommand],
  );

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

  const commands: AppCommands = {
    abortAll: () => runCommand(() => services.abortAll()),
    disconnect: async () => {
      await runCommand(() => services.disconnectLv1());
      // Disconnect is an explicit connection-management action; keep the modal
      // open so the engineer can immediately choose another console.
      setConnectionModalMode("manual");
    },
    addSceneToActiveCueList: (sceneInternalId, insertIndex) =>
      runCommand(() =>
        services.addSceneToActiveCueList(sceneInternalId, insertIndex),
      ),
    createCueList: (name) => runCommand(() => services.createCueList(name)),
    copySceneSettings: (internalSceneId) =>
      runCommand(() => services.copySceneSettings(internalSceneId)),
    cueEntry: (cueEntryId) => runCommand(() => services.cueEntry(cueEntryId)),
    deleteCueList: (cueListId) =>
      runCommand(() => services.deleteCueList(cueListId)),
    newShowFile: () => runCommand(() => services.newShowFile()),
    openShowFile: () => runCommand(() => services.openShowFile()),
    pasteSceneSettings: (internalSceneId) =>
      runCommand(() => services.pasteSceneSettings(internalSceneId)),
    removeCueEntry: (cueEntryId) =>
      runCommand(() => services.removeCueEntry(cueEntryId)),
    recallCuedCue: () => runCommand(() => services.recallCuedCue()),
    renameCueList: (cueListId, name) =>
      runCommand(() => services.renameCueList(cueListId, name)),
    reorderCueEntries: (orderedEntryIds) =>
      runCommand(() => services.reorderCueEntries(orderedEntryIds)),
    reorderCueLists: (orderedIds) =>
      runCommand(() => services.reorderCueLists(orderedIds)),
    linkSceneConfig: (sourceInternalSceneId, targetSceneIndex, overwrite) =>
      runCommand(() =>
        services.linkSceneConfig(
          sourceInternalSceneId,
          targetSceneIndex,
          overwrite,
        ),
      ),
    deleteSceneConfig: (internalSceneId) =>
      runCommand(() => services.deleteSceneConfig(internalSceneId)),
    recallScene: (internalSceneId) =>
      runCommand(() => services.recallScene(internalSceneId)),
    probeLv1TcpConnectLatency: (identity, timeoutMs) =>
      services.probeLv1TcpConnectLatency(identity, timeoutMs),
    saveShowFile: () => runCommand(() => services.saveShowFile()),
    saveShowFileAs: () => runCommand(() => services.saveShowFileAs()),
    selectScene: (internalSceneId: string) =>
      runCommand(() => services.selectSceneConfig(internalSceneId)),
    setActiveCueList: (cueListId) =>
      runCommand(() => services.setActiveCueList(cueListId)),
    selectSystem: (identity) =>
      runCommand(() => services.connectLv1System(identity)),
    setAllChannelsScoped: (internalSceneId: string, scoped: boolean) =>
      runCommand(() => services.setAllChannelsScoped(internalSceneId, scoped)),
    setChannelScoped: (internalSceneId, group, channel, scoped) =>
      runCommand(() =>
        services.setChannelScoped(internalSceneId, group, channel, scoped),
      ),
    setSceneDurationMs: (internalSceneId, durationMs) =>
      runCheckedCommand(() =>
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
      runCheckedCommand(() => services.storeSceneConfig(internalSceneId)),
    toggleLockout: () =>
      runCommand(() => services.setLockout(!appState.lockout)),
  };

  useEffect(() => {
    const title = formatSessionWindowTitle(
      appState.showFileName,
      appState.showFileDirty,
    );
    void services.setWindowTitle(title).catch((error) => {
      setCommandError(String(error));
    });
  }, [appState.showFileDirty, appState.showFileName, services]);

  return (
    <KeyboardProvider>
      <AppShortcutHandler appState={appState} commands={commands} />
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
    </KeyboardProvider>
  );
}

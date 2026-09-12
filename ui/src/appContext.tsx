import type { ReactNode } from "react";
import type {
  AppViewState,
  Lv1SystemIdentity,
  TcpConnectLatencyResult,
} from "./types";
import { AppCommandsContext, AppStateContext } from "./appContextValues";

type AppMutation = Promise<void>;

/**
 * @cc [owner:mixxorz,label:architecture] command-context-results
 * Context mutations MUST represent request completion rather than projected backend state;
 * operations that return `boolean` MUST use it only to report whether the request succeeded.
 */
export type AppCommands = {
  abortAll: () => AppMutation;
  addSceneToActiveCueList: (
    sceneInternalId: string,
    insertIndex: number,
  ) => AppMutation;
  cueEntry: (cueEntryId: string | null) => AppMutation;
  createCueList: (name: string) => AppMutation;
  copySceneSettings: (internalSceneId: string) => AppMutation;
  deleteCueList: (cueListId: string) => AppMutation;
  disconnect: () => AppMutation;
  newShowFile: () => AppMutation;
  openShowFile: () => AppMutation;
  pasteSceneSettings: (internalSceneId: string) => AppMutation;
  removeCueEntry: (cueEntryId: string) => AppMutation;
  recallCuedCue: () => AppMutation;
  renameCueList: (cueListId: string, name: string) => AppMutation;
  linkSceneConfig: (
    sourceInternalSceneId: string,
    targetSceneIndex: number,
    overwriteExisting: boolean,
  ) => AppMutation;
  reorderCueEntries: (orderedEntryIds: string[]) => AppMutation;
  reorderCueLists: (orderedIds: string[]) => AppMutation;
  deleteSceneConfig: (internalSceneId: string) => AppMutation;
  setActiveCueList: (cueListId: string | null) => AppMutation;
  saveShowFile: () => AppMutation;
  saveShowFileAs: () => AppMutation;
  selectScene: (internalSceneId: string) => AppMutation;
  recallScene: (internalSceneId: string) => AppMutation;
  selectSystem: (identity: Lv1SystemIdentity) => AppMutation;
  probeLv1TcpConnectLatency: (
    identity: Lv1SystemIdentity,
    timeoutMs?: number,
  ) => Promise<TcpConnectLatencyResult>;
  setAllChannelsScoped: (
    internalSceneId: string,
    scoped: boolean,
  ) => AppMutation;
  setChannelScoped: (
    internalSceneId: string,
    group: number,
    channel: number,
    scoped: boolean,
  ) => AppMutation;
  setSceneDurationMs: (
    internalSceneId: string,
    durationMs: number,
  ) => Promise<boolean>;
  setSceneScopeFadersEnabled: (
    internalSceneId: string,
    enabled: boolean,
  ) => AppMutation;
  setSceneScopePanEnabled: (
    internalSceneId: string,
    enabled: boolean,
  ) => AppMutation;
  storeSceneConfig: (internalSceneId: string) => Promise<boolean>;
  toggleLockout: () => AppMutation;
};

export type AppStateContextValue = {
  appState: AppViewState;
  commandError: string | null;
};

/**
 * @cc [owner:mixxorz,label:architecture] state-provider-passthrough
 * The state provider MUST expose the accepted `AppViewState` unchanged and MUST keep transient
 * command errors separate from that backend-owned snapshot.
 */
export function AppStateProvider(
  props: AppStateContextValue & { children: ReactNode },
) {
  return (
    <AppStateContext.Provider
      value={{ appState: props.appState, commandError: props.commandError }}
    >
      {props.children}
    </AppStateContext.Provider>
  );
}

/**
 * @cc [owner:mixxorz,label:architecture] commands-provider-required
 * The commands provider MUST publish an explicit complete `AppCommands` implementation; consumers
 * MUST NOT silently fall back to no-op or direct Tauri commands when a command is absent.
 */
export function AppCommandsProvider(props: {
  commands: AppCommands;
  children: ReactNode;
}) {
  return (
    <AppCommandsContext.Provider value={props.commands}>
      {props.children}
    </AppCommandsContext.Provider>
  );
}

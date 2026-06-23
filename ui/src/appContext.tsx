import type { ReactNode } from "react";
import type { AppViewState, Lv1SystemIdentity } from "./types";
import { AppCommandsContext, AppStateContext } from "./appContextValues";

export type AppCommands = {
  abortAll: () => void;
  cueScene?: (internalSceneId: string) => void;
  disconnect: () => void | Promise<void>;
  newShowFile: () => void;
  openShowFile: () => void;
  saveShowFile: () => void;
  saveShowFileAs: () => void;
  selectScene: (internalSceneId: string) => void;
  recallScene?: (internalSceneId: string) => void;
  selectSystem: (identity: Lv1SystemIdentity) => void | Promise<void>;
  setAllChannelsScoped: (internalSceneId: string, scoped: boolean) => void;
  setChannelScoped: (
    internalSceneId: string,
    group: number,
    channel: number,
    scoped: boolean,
  ) => void;
  setSceneDurationMs: (
    internalSceneId: string,
    durationMs: number,
  ) => Promise<boolean>;
  setSceneScopeFadersEnabled: (
    internalSceneId: string,
    enabled: boolean,
  ) => void;
  setSceneScopePanEnabled: (internalSceneId: string, enabled: boolean) => void;
  storeSceneConfig: (internalSceneId: string) => Promise<boolean>;
  toggleLockout: () => void;
};

export type AppStateContextValue = {
  appState: AppViewState;
  commandError: string | null;
};

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

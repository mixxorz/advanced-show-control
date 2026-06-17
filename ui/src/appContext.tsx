import { createContext, useContext, type ReactNode } from "react";
import type { AppViewState, Lv1SystemIdentity } from "./types";

export type AppCommands = {
  abortAll: () => void;
  disconnect: () => void | Promise<void>;
  newShowFile: () => void;
  openShowFile: () => void;
  saveShowFile: () => void;
  saveShowFileAs: () => void;
  selectScene: (sceneId: string) => void;
  selectSystem: (identity: Lv1SystemIdentity) => void | Promise<void>;
  setAllChannelsScoped: (sceneId: string, scoped: boolean) => void;
  setChannelScoped: (sceneId: string, group: number, channel: number, scoped: boolean) => void;
  setSceneDurationMs: (sceneId: string, durationMs: number) => Promise<boolean>;
  setSceneScopeFadersEnabled: (sceneId: string, enabled: boolean) => void;
  setSceneScopePanEnabled: (sceneId: string, enabled: boolean) => void;
  storeSceneConfig: (sceneId: string) => Promise<boolean>;
  toggleLockout: () => void;
};

type AppStateContextValue = {
  appState: AppViewState;
  commandError: string | null;
};

const AppStateContext = createContext<AppStateContextValue | null>(null);
const AppCommandsContext = createContext<AppCommands | null>(null);

export function AppStateProvider(props: AppStateContextValue & { children: ReactNode }) {
  return (
    <AppStateContext.Provider value={{ appState: props.appState, commandError: props.commandError }}>
      {props.children}
    </AppStateContext.Provider>
  );
}

export function AppCommandsProvider(props: { commands: AppCommands; children: ReactNode }) {
  return <AppCommandsContext.Provider value={props.commands}>{props.children}</AppCommandsContext.Provider>;
}

export function useAppState() {
  const value = useContext(AppStateContext);
  if (!value) {
    throw new Error("useAppState must be used within AppStateProvider");
  }
  return value;
}

export function useAppCommands() {
  const value = useContext(AppCommandsContext);
  if (!value) {
    throw new Error("useAppCommands must be used within AppCommandsProvider");
  }
  return value;
}

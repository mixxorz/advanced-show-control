import type { ReactNode } from "react";
import { AppCommandsProvider, AppStateProvider, type AppCommands } from "../appContext";
import { disconnectedAppViewState, type AppViewState } from "../types";

const noop = () => {};
const promiseTrue = async () => true;

export const mockAppCommands: AppCommands = {
  abortAll: noop,
  disconnect: noop,
  newShowFile: noop,
  openShowFile: noop,
  saveShowFile: noop,
  saveShowFileAs: noop,
  selectScene: noop,
  selectSystem: noop,
  setAllChannelsScoped: noop,
  setChannelScoped: noop,
  setSceneDurationMs: promiseTrue,
  setSceneScopeFadersEnabled: noop,
  setSceneScopePanEnabled: noop,
  storeSceneConfig: promiseTrue,
  toggleLockout: noop,
};

export function MockAppProviders(props: {
  appState?: AppViewState;
  commandError?: string | null;
  commands?: Partial<AppCommands>;
  children: ReactNode;
}) {
  return (
    <AppStateProvider appState={props.appState ?? disconnectedAppViewState} commandError={props.commandError ?? null}>
      <AppCommandsProvider commands={{ ...mockAppCommands, ...props.commands }}>{props.children}</AppCommandsProvider>
    </AppStateProvider>
  );
}

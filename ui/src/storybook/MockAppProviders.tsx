import type { ReactNode } from "react";
import {
  AppCommandsProvider,
  AppStateProvider,
  type AppCommands,
} from "../appContext";
import { KeyboardProvider } from "../keyboard";
import { disconnectedAppViewState, type AppViewState } from "../types";
import { mockAppCommands } from "./mockAppCommands";

/**
 * @cc [owner:mixxorz,label:testing] story-command-overrides-are-local
 * The provider MUST use deterministic default commands and replace only keys explicitly supplied
 * by the caller; omitted overrides MUST NOT reach real Tauri commands or inherit prior renders.
 */
export function MockAppProviders(props: {
  appState?: AppViewState;
  commandError?: string | null;
  commands?: Partial<AppCommands>;
  children: ReactNode;
}) {
  return (
    <KeyboardProvider>
      <AppStateProvider
        appState={props.appState ?? disconnectedAppViewState}
        commandError={props.commandError ?? null}
      >
        <AppCommandsProvider
          commands={{ ...mockAppCommands, ...props.commands }}
        >
          {props.children}
        </AppCommandsProvider>
      </AppStateProvider>
    </KeyboardProvider>
  );
}

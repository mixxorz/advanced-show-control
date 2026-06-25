import type { ReactNode } from "react";
import {
  AppCommandsProvider,
  AppStateProvider,
  type AppCommands,
} from "../appContext";
import { KeyboardProvider } from "../keyboard";
import { disconnectedAppViewState, type AppViewState } from "../types";
import { mockAppCommands } from "./mockAppCommands";

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

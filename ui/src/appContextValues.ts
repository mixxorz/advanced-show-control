import { createContext } from "react";
import type { AppCommands, AppStateContextValue } from "./appContext";

/**
 * @cc [owner:mixxorz,label:architecture] nullable-provider-sentinel
 * The default MUST remain `null` so consumers outside `AppStateProvider` fail explicitly rather
 * than observing a fabricated backend snapshot.
 */
export const AppStateContext = createContext<AppStateContextValue | null>(null);

/**
 * @cc [owner:mixxorz,label:architecture] nullable-commands-sentinel
 * The default MUST remain `null` so consumers outside `AppCommandsProvider` fail explicitly rather
 * than issuing no-op or partially wired mutations.
 */
export const AppCommandsContext = createContext<AppCommands | null>(null);

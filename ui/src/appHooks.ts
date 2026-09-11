import { useContext } from "react";
import { AppCommandsContext, AppStateContext } from "./appContextValues";

/**
 * @cc [owner:mixxorz,label:architecture] app-state-provider-required
 * Calling this hook outside `AppStateProvider` MUST throw; it MUST NOT return defaults that could
 * be mistaken for an accepted backend snapshot.
 */
export function useAppState() {
  const value = useContext(AppStateContext);
  if (!value) {
    throw new Error("useAppState must be used within AppStateProvider");
  }
  return value;
}

/**
 * @cc [owner:mixxorz,label:architecture] app-commands-provider-required
 * Calling this hook outside `AppCommandsProvider` MUST throw; it MUST NOT return fallback commands.
 */
export function useAppCommands() {
  const value = useContext(AppCommandsContext);
  if (!value) {
    throw new Error("useAppCommands must be used within AppCommandsProvider");
  }
  return value;
}

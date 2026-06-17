import { useContext } from "react";
import { AppCommandsContext, AppStateContext } from "./appContextValues";

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

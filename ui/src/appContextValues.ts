import { createContext } from "react";
import type { AppCommands, AppStateContextValue } from "./appContext";

export const AppStateContext = createContext<AppStateContextValue | null>(null);
export const AppCommandsContext = createContext<AppCommands | null>(null);

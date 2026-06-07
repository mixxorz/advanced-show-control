import { invoke } from "@tauri-apps/api/core";
import type { AppViewState } from "./types";

export async function refreshAppState(
  setAppState: (appState: AppViewState) => void,
  setCommandError: (message: string | null) => void,
) {
  try {
    setAppState(await invoke<AppViewState>("get_app_status"));
  } catch (error) {
    setCommandError(String(error));
  }
}

export async function runSnapshotCommand(
  command: string,
  args: Record<string, unknown> | undefined,
  setAppState: (appState: AppViewState) => void,
  setCommandError: (message: string | null) => void,
) {
  setCommandError(null);
  try {
    const next = await invoke<AppViewState>(command, args);
    setAppState(next);
    return true;
  } catch (error) {
    setCommandError(String(error));
    await refreshAppState(setAppState, setCommandError);
    return false;
  }
}

export async function runVoidCommand(
  command: string,
  setAppState: (appState: AppViewState) => void,
  setCommandError: (message: string | null) => void,
) {
  setCommandError(null);
  try {
    await invoke(command);
    await refreshAppState(setAppState, setCommandError);
  } catch (error) {
    setCommandError(String(error));
  }
}

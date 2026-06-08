import { invoke } from "@tauri-apps/api/core";
import type { AppViewState, Lv1SystemIdentity } from "./types";

export async function startupAutoConnectLv1() {
  return invoke<AppViewState>("startup_auto_connect_lv1");
}

export async function refreshLv1Discovery() {
  return invoke<AppViewState>("refresh_lv1_discovery", { timeoutMs: 1000 });
}

export async function connectLv1System(identity: Lv1SystemIdentity) {
  return invoke<AppViewState>("connect_lv1_system", { identity });
}

export async function reconnectTimedOut(attempt: number) {
  return invoke<AppViewState>("reconnect_timed_out", { attempt });
}

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

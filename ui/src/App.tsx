import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AppRuntime, type AppRuntimeServices } from "./AppRuntime";
import * as commands from "./commands";
import type { AppViewState } from "./types";

/**
 * @cc [owner:mixxorz,label:architecture] production-runtime-boundary
 * Production `listenForAppStatus` MUST subscribe to `app-status-changed` and forward each event
 * payload unchanged; request services MUST use the Tauri command bridge.
 */
const services: AppRuntimeServices = {
  ...commands,
  listenForAppStatus: (listener) =>
    listen<AppViewState>("app-status-changed", (event) =>
      listener(event.payload),
    ),
  setWindowTitle: (title) => getCurrentWindow().setTitle(title),
};

/**
 * @cc [owner:mixxorz,label:architecture] production-service-injection
 * The production app MUST mount `AppRuntime` with the Tauri-backed service set rather than a
 * frontend-owned backend-state implementation.
 */
export default function App() {
  return <AppRuntime services={services} />;
}

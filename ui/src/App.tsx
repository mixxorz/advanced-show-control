import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AppRuntime, type AppRuntimeServices } from "./AppRuntime";
import * as commands from "./commands";
import type { AppViewState } from "./types";

const services: AppRuntimeServices = {
  ...commands,
  listenForAppStatus: (listener) =>
    listen<AppViewState>("app-status-changed", (event) =>
      listener(event.payload),
    ),
  setWindowTitle: (title) => getCurrentWindow().setTitle(title),
};

export default function App() {
  return <AppRuntime services={services} />;
}

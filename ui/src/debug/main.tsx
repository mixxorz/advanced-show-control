import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import React from "react";
import { createRoot } from "react-dom/client";
import type { AppViewState } from "../types";
import "../index.css";
import { SmokeDebugApp } from "./SmokeDebugApp";

const services = {
  frontendReady: () => invoke<void>("frontend_ready"),
  listenForAppStatus: (listener: (snapshot: AppViewState) => void) =>
    listen<AppViewState>("app-status-changed", (event) =>
      listener(event.payload),
    ),
};

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <SmokeDebugApp services={services} />
  </React.StrictMode>,
);

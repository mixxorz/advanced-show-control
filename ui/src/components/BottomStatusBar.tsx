import { useEffect, useState } from "react";
import type { AppViewState } from "../types";
import { formatSceneNumber } from "../format";
import { StatusCell } from "./StatusCell";

function formatClock(date: Date) {
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

function selectedSceneLabel(appState: AppViewState) {
  const selected = appState.sceneConfigs.find(
    (scene) => scene.sceneId === appState.selectedSceneId,
  );
  return selected
    ? `${formatSceneNumber(selected.sceneIndex)} ${selected.sceneName}`
    : "None";
}

export function BottomStatusBar(props: { appState: AppViewState }) {
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const currentScene = props.appState.currentScene
    ? `${formatSceneNumber(props.appState.currentScene.index)} ${props.appState.currentScene.name}`
    : "None";
  const connection = props.appState.connectedLv1Identity?.host
    ? `Connected to ${props.appState.connectedLv1Identity.host}`
    : props.appState.connection;
  const modeTone = props.appState.lockout
    ? "warning"
    : props.appState.fadeState === "blocked"
      ? "danger"
      : "default";
  const syncValue = props.appState.reconnect.active
    ? "Reconnecting"
    : props.appState.connection === "connected"
      ? "In Sync"
      : "Offline";
  const syncTone = props.appState.reconnect.active
    ? "warning"
    : props.appState.connection === "connected"
      ? "current"
      : "danger";

  return (
    <footer className="grid grid-cols-1 border-t border-console-line bg-console-chrome md:grid-cols-[0.8fr_1.2fr_1.4fr_1.8fr_1fr_1fr]">
      <StatusCell
        label="Mode"
        tone={modeTone}
        value={
          props.appState.lockout
            ? "LOCKOUT"
            : props.appState.fadeState.toUpperCase()
        }
      />
      <StatusCell label="Current" tone="current" value={currentScene} />
      <StatusCell
        label="Selected"
        tone="cued"
        value={selectedSceneLabel(props.appState)}
      />
      <StatusCell
        label="Connection"
        tone={props.appState.connection === "connected" ? "current" : "danger"}
        value={connection}
      />
      <StatusCell label="Sync" tone={syncTone} value={syncValue} />
      <StatusCell label="Time" value={formatClock(now)} />
    </footer>
  );
}

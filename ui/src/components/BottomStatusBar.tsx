import { useEffect, useState } from "react";
import type { AppViewState } from "../types";
import { useAppCommands } from "../appHooks";
import { ConsoleButton } from "./ConsoleButton";
import { StatusCell } from "./StatusCell";

function formatClock(date: Date) {
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

function cuedSceneLabel(appState: AppViewState) {
  const cued = appState.sceneConfigs.find(
    (scene) => scene.sceneId === appState.cuedSceneId,
  );
  return cued ? cued.sceneName : "---";
}

function modeDisplay(appState: AppViewState): {
  className?: string;
  tone: "default" | "cued" | "warning";
  value: string;
} {
  if (appState.connection !== "connected") {
    return { tone: "default", value: "Offline" };
  }

  if (appState.lockout) {
    return { tone: "warning", value: "Safe" };
  }

  if (appState.fadeState === "running") {
    return { className: "animate-pulse", tone: "warning", value: "Fading" };
  }

  return { tone: "cued", value: "Ready" };
}

export function BottomStatusBar(props: { appState: AppViewState }) {
  const commands = useAppCommands();
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const currentScene = props.appState.currentScene
    ? props.appState.currentScene.name
    : "---";
  const mode = modeDisplay(props.appState);
  const canGo = Boolean(props.appState.cuedSceneId && commands.recallScene);

  return (
    <footer className="mx-3 mb-3 grid grid-cols-1 overflow-hidden rounded-console-panel border border-console-line bg-console-chrome md:grid-cols-[0.7fr_1.4fr_1.4fr_0.9fr_0.8fr]">
      <div className="grid min-w-0 place-items-center border-r border-console-line p-3 last:border-r-0">
        <ConsoleButton
          disabled={!canGo}
          fullWidth
          onClick={() => {
            if (props.appState.cuedSceneId) {
              commands.recallScene?.(props.appState.cuedSceneId);
            }
          }}
          size="big"
          variant="primary"
        >
          GO
        </ConsoleButton>
      </div>
      <StatusCell
        label="Cued"
        tone={props.appState.cuedSceneId ? "cued" : "default"}
        value={cuedSceneLabel(props.appState)}
      />
      <StatusCell label="Current" tone="current" value={currentScene} />
      <StatusCell
        label="Mode"
        className={mode.className}
        tone={mode.tone}
        value={mode.value}
      />
      <StatusCell font="mono" label="Time" value={formatClock(now)} />
    </footer>
  );
}

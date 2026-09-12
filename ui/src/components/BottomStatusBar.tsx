import { useEffect, useRef, useState } from "react";
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

/**
 * @cc [owner:mixxorz,label:product] cued-scene-resolution
 * A cued scene MUST resolve through the active cue list, then the cued entry, then its referenced
 * scene config; any missing link MUST produce no scene rather than falling back to selection.
 */
function resolveCuedScene(appState: AppViewState) {
  const activeCueList = appState.cueLists.find(
    (cueList) => cueList.id === appState.activeCueListId,
  );
  const cuedCueEntry = activeCueList?.entries.find(
    (entry) => entry.id === appState.cuedCueEntryId,
  );
  return cuedCueEntry
    ? (appState.sceneConfigs.find(
        (scene) => scene.internalSceneId === cuedCueEntry.sceneInternalId,
      ) ?? null)
    : null;
}

/**
 * @cc [owner:mixxorz,label:product] status-mode-precedence
 * Mode MUST display Offline when disconnected, otherwise Safe during lockout, otherwise Fading
 * while a fade runs, and Ready only when none of those higher-priority states applies.
 */
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

/**
 * @cc [owner:mixxorz,label:safety;product] go-single-flight
 * GO MUST be disabled without a resolvable cued scene and while a recall is pending, MUST submit at
 * most one recall concurrently, and MUST clear its pending guard after either success or failure.
 */
export function BottomStatusBar(props: { appState: AppViewState }) {
  const commands = useAppCommands();
  const [now, setNow] = useState(() => new Date());
  const [goPending, setGoPending] = useState(false);
  const goPendingRef = useRef(false);

  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const cuedScene = resolveCuedScene(props.appState);
  const currentScene = props.appState.currentScene
    ? props.appState.currentScene.name
    : "---";
  const mode = modeDisplay(props.appState);
  const canGo = cuedScene !== null && !goPending;

  async function handleGo() {
    if (!canGo || goPendingRef.current) {
      return;
    }

    goPendingRef.current = true;
    setGoPending(true);

    try {
      await commands.recallCuedCue();
    } catch {
      // The command error is surfaced elsewhere; the footer only clears the guard.
    } finally {
      goPendingRef.current = false;
      setGoPending(false);
    }
  }

  return (
    <footer className="mx-3 mb-3 grid grid-cols-1 overflow-hidden rounded-console-panel border border-console-line bg-console-chrome md:grid-cols-[0.7fr_1.4fr_1.4fr_0.9fr_0.8fr]">
      <div className="grid min-w-0 place-items-center border-r border-console-line p-3 last:border-r-0">
        <ConsoleButton
          disabled={!canGo}
          fullWidth
          onClick={() => {
            void handleGo();
          }}
          size="big"
          variant="primary"
        >
          GO
        </ConsoleButton>
      </div>
      <StatusCell
        label="Cued"
        tone={cuedScene ? "cued" : "default"}
        value={cuedScene?.sceneName ?? "---"}
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

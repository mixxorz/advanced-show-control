import { useAppCommands, useAppState } from "../appHooks";
import { formatSceneNumber } from "../format";
import { ShowFileControls } from "./ShowFileControls";
import { StatusBadge } from "./StatusBadge";

export function Header(props: { onOpenConnection: () => void }) {
  const { appState, commandError } = useAppState();
  const commands = useAppCommands();

  return (
    <header className="border-b border-slate-800 bg-slate-900/80 px-6 py-4">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold">Advanced Show Control</h1>
          <p className="text-sm text-slate-400">
            {appState.currentScene
              ? `Scene ${formatSceneNumber(appState.currentScene.index)}: ${appState.currentScene.name}`
              : "No LV1 scene selected"}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <ShowFileControls
            dirty={appState.showFileDirty}
            fileName={appState.showFileName}
            filePath={appState.showFilePath}
            onNew={commands.newShowFile}
            onOpen={commands.openShowFile}
            onSave={commands.saveShowFile}
            onSaveAs={commands.saveShowFileAs}
          />
          <button
            aria-label="Open LV1 connection screen"
            onClick={props.onOpenConnection}
            className="rounded-full focus:outline-none focus:ring-2 focus:ring-cyan-400"
          >
            <StatusBadge
              label={appState.connection}
              tone={appState.connection === "connected" ? "good" : "neutral"}
            />
          </button>
          <StatusBadge
            label={`Fade: ${appState.fadeState}`}
            tone={appState.fadeState === "blocked" ? "warning" : "neutral"}
          />
          <button
            className={
              appState.lockout
                ? "rounded-full border border-amber-500/60 bg-amber-950 px-3 py-1 text-sm text-amber-100"
                : "rounded-full border border-slate-700 bg-slate-800 px-3 py-1 text-sm text-slate-200"
            }
            onClick={commands.toggleLockout}
          >
            {appState.lockout ? "Lockout On" : "Lockout Off"}
          </button>
          <button
            className="rounded-lg bg-red-700 px-5 py-3 font-bold text-white shadow-lg shadow-red-950/40 hover:bg-red-600"
            onClick={commands.abortAll}
          >
            Abort All
          </button>
        </div>
      </div>
      {commandError && (
        <p className="mt-3 rounded-lg border border-red-800 bg-red-950 px-3 py-2 text-sm text-red-100">
          {commandError}
        </p>
      )}
    </header>
  );
}

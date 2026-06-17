import type { ReactNode } from "react";
import { useAppCommands, useAppState } from "../appContext";
import { ConnectionScreen } from "./ConnectionScreen";
import { Header } from "./Header";
import { LogsTab } from "./LogsTab";
import { SceneTab } from "./SceneTab";

export type MainTab = "scene" | "logs";

export function AppShell(props: {
  activeTab: MainTab;
  onOpenConnection: () => void;
  onResume: () => void;
  onSelectTab: (tab: MainTab) => void;
  showConnection: boolean;
}) {
  const { appState, commandError } = useAppState();
  const commands = useAppCommands();

  return (
    <>
      {props.showConnection ? (
        <ConnectionScreen appState={appState} commandError={commandError} onDisconnect={commands.disconnect} onResume={props.onResume} onSelectSystem={commands.selectSystem} />
      ) : (
        <main className="min-h-screen bg-slate-950 text-slate-100">
          <Header appState={appState} commandError={commandError} onAbortAll={commands.abortAll} onNewShowFile={commands.newShowFile} onOpenConnection={props.onOpenConnection} onOpenShowFile={commands.openShowFile} onSaveShowFile={commands.saveShowFile} onSaveShowFileAs={commands.saveShowFileAs} onToggleLockout={commands.toggleLockout} />

          <nav className="border-b border-slate-800 px-6">
            <div className="flex gap-2">
              <TabButton active={props.activeTab === "scene"} onClick={() => props.onSelectTab("scene")}>
                Scene
              </TabButton>
              <TabButton active={props.activeTab === "logs"} onClick={() => props.onSelectTab("logs")}>
                Logs
              </TabButton>
            </div>
          </nav>

          <section className="p-6">
            {props.activeTab === "scene" && (
              <SceneTab appState={appState} selectScene={commands.selectScene} setSceneDurationMs={commands.setSceneDurationMs} setSceneScopeFadersEnabled={commands.setSceneScopeFadersEnabled} setSceneScopePanEnabled={commands.setSceneScopePanEnabled} storeSceneConfig={commands.storeSceneConfig} setAllChannelsScoped={commands.setAllChannelsScoped} setChannelScoped={commands.setChannelScoped} />
            )}
            {props.activeTab === "logs" && <LogsTab appState={appState} />}
          </section>
        </main>
      )}

      <ReconnectOverlay active={appState.reconnect.active} />
    </>
  );
}

function ReconnectOverlay(props: { active: boolean }) {
  if (!props.active) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/70">
      <div className="rounded-xl border border-slate-700 bg-slate-900 px-8 py-6 text-xl font-semibold text-slate-100 shadow-2xl">
        Reconnecting...
      </div>
    </div>
  );
}

function TabButton(props: { active: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      className={
        props.active
          ? "border-b-2 border-cyan-400 px-4 py-3 text-cyan-200"
          : "px-4 py-3 text-slate-400 hover:text-slate-100"
      }
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}

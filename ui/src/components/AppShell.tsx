import type { ReactNode } from "react";
import type { AppViewState, Lv1SystemIdentity } from "../types";
import { ConnectionScreen } from "./ConnectionScreen";
import { Header } from "./Header";
import { LogsTab } from "./LogsTab";
import { SceneTab } from "./SceneTab";

export type MainTab = "scene" | "logs";

export function AppShell(props: {
  activeTab: MainTab;
  appState: AppViewState;
  commandError: string | null;
  onAbortAll: () => void;
  onDisconnect: () => void;
  onNewShowFile: () => void;
  onOpenConnection: () => void;
  onOpenShowFile: () => void;
  onResume: () => void;
  onSaveShowFile: () => void;
  onSaveShowFileAs: () => void;
  onSelectScene: (sceneId: string) => void;
  onSelectSystem: (identity: Lv1SystemIdentity) => void;
  onSelectTab: (tab: MainTab) => void;
  onSetAllChannelsScoped: (sceneId: string, scoped: boolean) => void;
  onSetChannelScoped: (sceneId: string, group: number, channel: number, scoped: boolean) => void;
  onSetSceneDurationMs: (sceneId: string, durationMs: number) => Promise<boolean>;
  onSetSceneScopeFadersEnabled: (sceneId: string, enabled: boolean) => void;
  onSetSceneScopePanEnabled: (sceneId: string, enabled: boolean) => void;
  onStoreSceneConfig: (sceneId: string) => Promise<boolean>;
  onToggleLockout: () => void;
  showConnection: boolean;
}) {
  return (
    <>
      {props.showConnection ? (
        <ConnectionScreen
          appState={props.appState}
          commandError={props.commandError}
          onDisconnect={props.onDisconnect}
          onResume={props.onResume}
          onSelectSystem={props.onSelectSystem}
        />
      ) : (
        <main className="min-h-screen bg-slate-950 text-slate-100">
          <Header
            appState={props.appState}
            commandError={props.commandError}
            onAbortAll={props.onAbortAll}
            onNewShowFile={props.onNewShowFile}
            onOpenConnection={props.onOpenConnection}
            onOpenShowFile={props.onOpenShowFile}
            onSaveShowFile={props.onSaveShowFile}
            onSaveShowFileAs={props.onSaveShowFileAs}
            onToggleLockout={props.onToggleLockout}
          />

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
              <SceneTab
                appState={props.appState}
                selectScene={props.onSelectScene}
                setSceneDurationMs={props.onSetSceneDurationMs}
                setSceneScopeFadersEnabled={props.onSetSceneScopeFadersEnabled}
                setSceneScopePanEnabled={props.onSetSceneScopePanEnabled}
                storeSceneConfig={props.onStoreSceneConfig}
                setAllChannelsScoped={props.onSetAllChannelsScoped}
                setChannelScoped={props.onSetChannelScoped}
              />
            )}
            {props.activeTab === "logs" && <LogsTab appState={props.appState} />}
          </section>
        </main>
      )}

      <ReconnectOverlay active={props.appState.reconnect.active} />
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

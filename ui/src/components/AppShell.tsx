import { useAppState } from "../appHooks";
import { BottomStatusBar } from "./BottomStatusBar";
import { ConnectionScreen } from "./ConnectionScreen";
import { ConsoleLogsTab } from "./ConsoleLogsTab";
import { PlaceholderTab } from "./PlaceholderTab";
import { SceneTab } from "./SceneTab";
import { type MainTab, TopTabBar } from "./TopTabBar";

export type { MainTab } from "./TopTabBar";

export function AppShell(props: {
  activeTab: MainTab;
  onOpenConnection: () => void;
  onResume: () => void;
  onSelectTab: (tab: MainTab) => void;
  showConnection: boolean;
}) {
  const { appState } = useAppState();

  return (
    <>
      {props.showConnection ? (
        <ConnectionScreen onResume={props.onResume} />
      ) : (
        <main className="grid h-screen grid-rows-[auto_1fr_auto] overflow-hidden bg-black font-ui text-console-primary">
          <TopTabBar
            activeTab={props.activeTab}
            onSelectTab={props.onSelectTab}
          />
          <section className="min-h-0 overflow-hidden p-3">
            {props.activeTab === "scenes" && <SceneTab />}
            {props.activeTab === "playlists" && (
              <PlaceholderTab name="Cue Lists" />
            )}
            {props.activeTab === "events" && <PlaceholderTab name="Events" />}
            {props.activeTab === "sessions" && (
              <PlaceholderTab name="Sessions" />
            )}
            {props.activeTab === "logs" && <ConsoleLogsTab />}
            {props.activeTab === "settings" && (
              <PlaceholderTab name="Settings" />
            )}
          </section>
          <BottomStatusBar appState={appState} />
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
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70">
      <div className="rounded-xl border border-console-line bg-console-panel px-8 py-6 text-xl font-semibold text-console-primary shadow-2xl">
        Reconnecting...
      </div>
    </div>
  );
}

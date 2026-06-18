import { useAppState } from "../appHooks";
import { TopTab } from "./TopTab";

export type MainTab =
  | "scenes"
  | "playlists"
  | "events"
  | "sessions"
  | "logs"
  | "settings";

const tabs: { id: MainTab; label: string }[] = [
  { id: "scenes", label: "Scenes" },
  { id: "playlists", label: "Cue Lists" },
  { id: "events", label: "Events" },
  { id: "sessions", label: "Sessions" },
  { id: "logs", label: "Logs" },
  { id: "settings", label: "Settings" },
];

export function TopTabBar(props: {
  activeTab: MainTab;
  onSelectTab: (tab: MainTab) => void;
}) {
  const { appState } = useAppState();
  const connected = appState.connection === "connected";
  const consoleName = appState.connectedLv1Identity?.host ?? "Console A";

  return (
    <nav className="mx-3 mt-3 flex overflow-hidden rounded-console-panel border border-console-line bg-console-chrome">
      <div className="flex min-w-0 flex-1">
        {tabs.map((tab) => (
          <TopTab
            active={props.activeTab === tab.id}
            key={tab.id}
            onClick={() => props.onSelectTab(tab.id)}
          >
            {tab.label}
          </TopTab>
        ))}
      </div>
      <div className="flex items-center gap-5 px-4">
        <div
          className={
            connected
              ? "flex items-center gap-2 font-mono text-sm font-normal uppercase text-status-cued"
              : "flex items-center gap-2 font-mono text-sm font-normal uppercase text-status-danger"
          }
        >
          <span
            className={
              connected
                ? "h-2.5 w-2.5 rounded-full bg-status-cued"
                : "h-2.5 w-2.5 rounded-full bg-status-danger"
            }
          />
          {connected ? "Connected" : "Offline"}
        </div>
        <button className="flex min-w-36 items-center justify-between gap-4 rounded-console-control border border-console-line bg-black/20 px-3 py-2 text-base font-normal uppercase text-console-primary shadow-inner hover:border-console-line-strong">
          <span className="truncate">{consoleName}</span>
          <span className="h-0 w-0 border-x-[5px] border-t-[6px] border-x-transparent border-t-console-secondary" />
        </button>
      </div>
    </nav>
  );
}

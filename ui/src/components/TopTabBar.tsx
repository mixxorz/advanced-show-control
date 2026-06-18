import { useAppCommands, useAppState } from "../appHooks";
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
  onOpenConnection: () => void;
  onSelectTab: (tab: MainTab) => void;
}) {
  const { appState } = useAppState();
  const commands = useAppCommands();
  const connected = appState.connection === "connected";
  const connecting = appState.connection === "connecting";
  const consoleName = appState.connectedLv1Identity?.host ?? "Console A";
  const statusLabel = connected
    ? "Connected"
    : connecting
      ? "Connecting"
      : "Offline";
  const statusClass = connected
    ? "text-status-cued"
    : connecting
      ? "text-console-secondary"
      : "text-status-danger";
  const dotClass = connected
    ? "bg-status-cued"
    : connecting
      ? "bg-console-secondary"
      : "bg-status-danger";
  const safeClass = appState.lockout
    ? "border-status-warning bg-status-warning/15 text-status-warning shadow-inner shadow-status-warning/20"
    : "border-console-line bg-black/20 text-console-primary hover:border-console-line-strong";

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
      <div className="flex items-center gap-3 px-4">
        <button
          aria-pressed={appState.lockout}
          className={`rounded-console-control border px-3 py-2 font-mono text-sm font-normal uppercase ${safeClass}`}
          onClick={commands.toggleLockout}
          type="button"
        >
          SAFE
        </button>
        <div
          className={`flex items-center gap-2 font-mono text-sm font-normal uppercase ${statusClass}`}
        >
          <span className={`h-2.5 w-2.5 rounded-full ${dotClass}`} />
          {statusLabel}
        </div>
        <button
          className="flex min-w-36 items-center justify-between gap-4 rounded-console-control border border-console-line bg-black/20 px-3 py-2 text-base font-normal uppercase text-console-primary shadow-inner hover:border-console-line-strong"
          onClick={props.onOpenConnection}
          type="button"
        >
          <span className="truncate">{consoleName}</span>
          <span className="h-0 w-0 border-x-[5px] border-t-[6px] border-x-transparent border-t-console-secondary" />
        </button>
      </div>
    </nav>
  );
}

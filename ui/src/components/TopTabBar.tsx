import { useAppCommands, useAppState } from "../appHooks";
import { ConsoleButton } from "./ConsoleButton";
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
        <div
          className={`flex items-center gap-2 font-mono text-sm font-normal uppercase ${statusClass}`}
        >
          <span className={`h-2.5 w-2.5 rounded-full ${dotClass}`} />
          {statusLabel}
        </div>
        <ConsoleButton
          className="flex min-w-36 items-center justify-between gap-4 shadow-inner"
          onClick={props.onOpenConnection}
          variant="secondary"
        >
          <span className="truncate">{consoleName}</span>
          <span className="h-0 w-0 border-x-[5px] border-t-[6px] border-x-transparent border-t-console-secondary" />
        </ConsoleButton>
        <ConsoleButton
          active={appState.lockout}
          ariaPressed={appState.lockout}
          className="font-mono"
          onClick={commands.toggleLockout}
          size="small"
          variant="warning"
        >
          SAFE
        </ConsoleButton>
      </div>
    </nav>
  );
}

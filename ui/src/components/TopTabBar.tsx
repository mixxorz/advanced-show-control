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
  { id: "playlists", label: "Playlists" },
  { id: "events", label: "Events" },
  { id: "sessions", label: "Sessions" },
  { id: "logs", label: "Logs" },
  { id: "settings", label: "Settings" },
];

export function TopTabBar(props: {
  activeTab: MainTab;
  onSelectTab: (tab: MainTab) => void;
}) {
  return (
    <nav className="flex border-b border-console-line bg-console-chrome">
      {tabs.map((tab) => (
        <TopTab
          active={props.activeTab === tab.id}
          key={tab.id}
          onClick={() => props.onSelectTab(tab.id)}
        >
          {tab.label}
        </TopTab>
      ))}
    </nav>
  );
}

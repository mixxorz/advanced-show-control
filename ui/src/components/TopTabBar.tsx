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
  return (
    <nav className="mx-3 mt-3 flex overflow-hidden rounded-console-panel border border-console-line bg-console-chrome">
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

import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import {
  connectedAppState,
  discoveringAppState,
} from "../storybook/mockAppState";
import { MockAppProviders } from "../storybook/MockAppProviders";
import type { AppViewState } from "../types";
import { TopTabBar } from "./TopTabBar";

function StatefulTopTabBarStory(props: {
  args: React.ComponentProps<typeof TopTabBar>;
  initialAppState: AppViewState;
}) {
  const [appState, setAppState] = useState(props.initialAppState);

  return (
    <MockAppProviders
      appState={appState}
      commands={{
        toggleLockout: () =>
          setAppState((state) => ({ ...state, lockout: !state.lockout })),
      }}
    >
      <TopTabBar {...props.args} />
    </MockAppProviders>
  );
}

const meta: Meta<typeof TopTabBar> = {
  title: "Shell/TopTabBar",
  component: TopTabBar,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    activeTab: "scenes",
    onOpenConnection: () => {},
    onSelectTab: () => {},
  },
  render: (args) => (
    <StatefulTopTabBarStory args={args} initialAppState={connectedAppState} />
  ),
};

export default meta;

type Story = StoryObj<typeof TopTabBar>;

export const Scenes: Story = {};

export const Logs: Story = {
  args: {
    activeTab: "logs",
  },
};

export const Offline: Story = {
  render: (args) => (
    <StatefulTopTabBarStory args={args} initialAppState={discoveringAppState} />
  ),
};

export const Connecting: Story = {
  render: (args) => (
    <StatefulTopTabBarStory
      args={args}
      initialAppState={{
        ...discoveringAppState,
        connection: "connecting",
      }}
    />
  ),
};

export const SafeActive: Story = {
  render: (args) => (
    <StatefulTopTabBarStory
      args={args}
      initialAppState={{
        ...connectedAppState,
        lockout: true,
      }}
    />
  ),
};

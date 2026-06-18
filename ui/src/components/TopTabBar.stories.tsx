import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  connectedAppState,
  discoveringAppState,
} from "../storybook/mockAppState";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { TopTabBar } from "./TopTabBar";

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
    <MockAppProviders appState={connectedAppState}>
      <TopTabBar {...args} />
    </MockAppProviders>
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
    <MockAppProviders appState={discoveringAppState}>
      <TopTabBar {...args} />
    </MockAppProviders>
  ),
};

export const Connecting: Story = {
  render: (args) => (
    <MockAppProviders
      appState={{
        ...discoveringAppState,
        connection: "connecting",
      }}
    >
      <TopTabBar {...args} />
    </MockAppProviders>
  ),
};

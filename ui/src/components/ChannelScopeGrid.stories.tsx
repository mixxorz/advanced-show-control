import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { connectedAppState, storedVerseScene } from "../storybook/mockAppState";
import { ChannelScopeGrid } from "./ChannelScopeGrid";

const emptyStoredScene = {
  ...storedVerseScene,
  channelConfigs: [],
  scopedChannels: [],
};

const meta: Meta<typeof ChannelScopeGrid> = {
  title: "Scenes/Channel Scope/ChannelScopeGrid",
  component: ChannelScopeGrid,
  parameters: {
    layout: "fullscreen",
  },
  decorators: [
    (Story) => (
      <main className="min-h-screen bg-console-bg p-6 text-console-primary">
        <MockAppProviders appState={connectedAppState}>
          <Story />
        </MockAppProviders>
      </main>
    ),
  ],
  args: {
    scene: storedVerseScene,
  },
};

export default meta;

type Story = StoryObj<typeof ChannelScopeGrid>;

export const Populated: Story = {};

export const Empty: Story = {
  args: {
    scene: emptyStoredScene,
  },
};

import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { connectedAppState, storedVerseScene } from "../storybook/mockAppState";
import { ChannelScopeToolbar } from "./ChannelScopeToolbar";

const meta: Meta<typeof ChannelScopeToolbar> = {
  title: "Scenes/Channel Scope/ChannelScopeToolbar",
  component: ChannelScopeToolbar,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <MockAppProviders appState={connectedAppState}>
          <Story />
        </MockAppProviders>
      </main>
    ),
  ],
  args: {
    allChannelsScoped: false,
    noChannelsScoped: false,
    internalSceneId: storedVerseScene.internalSceneId,
    scopeToggles: storedVerseScene.scopeToggles,
  },
};

export default meta;

type Story = StoryObj<typeof ChannelScopeToolbar>;

export const Default: Story = {};

export const AllSelected: Story = {
  args: {
    allChannelsScoped: true,
  },
};

export const NoneSelected: Story = {
  args: {
    noChannelsScoped: true,
  },
};

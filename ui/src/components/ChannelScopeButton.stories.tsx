import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import {
  connectedAppState,
  storedChorusScene,
  storedVerseScene,
} from "../storybook/mockAppState";
import { ChannelScopeButton } from "./ChannelScopeButton";

const meta: Meta<typeof ChannelScopeButton> = {
  title: "Scenes/Channel Scope/ChannelScopeButton",
  component: ChannelScopeButton,
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
    config: storedVerseScene.channelConfigs[0],
    sceneId: storedVerseScene.sceneId,
    scoped: true,
  },
};

export default meta;

type Story = StoryObj<typeof ChannelScopeButton>;

export const Scoped: Story = {};

export const UnscopedStereo: Story = {
  args: {
    config: storedChorusScene.channelConfigs[2],
    sceneId: storedChorusScene.sceneId,
    scoped: false,
  },
};

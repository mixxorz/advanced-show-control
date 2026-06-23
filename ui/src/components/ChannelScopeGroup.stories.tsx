import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { connectedAppState, storedVerseScene } from "../storybook/mockAppState";
import { ChannelScopeGroup } from "./ChannelScopeGroup";

const meta: Meta<typeof ChannelScopeGroup> = {
  title: "Scenes/Channel Scope/ChannelScopeGroup",
  component: ChannelScopeGroup,
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
    configs: storedVerseScene.channelConfigs.filter(
      (config) => config.group === 0,
    ),
    groupName: "Inputs",
    sceneId: storedVerseScene.internalSceneId,
    scoped: new Set(["0:0", "0:2"]),
  },
};

export default meta;

type Story = StoryObj<typeof ChannelScopeGroup>;

export const Inputs: Story = {};

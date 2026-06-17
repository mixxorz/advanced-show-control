import type { Meta, StoryObj } from "@storybook/react-vite";
import { ChannelScopeEmptyState } from "./ChannelScopeEmptyState";

const meta: Meta<typeof ChannelScopeEmptyState> = {
  title: "Scenes/Channel Scope/ChannelScopeEmptyState",
  component: ChannelScopeEmptyState,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
};

export default meta;

type Story = StoryObj<typeof ChannelScopeEmptyState>;

export const Default: Story = {};

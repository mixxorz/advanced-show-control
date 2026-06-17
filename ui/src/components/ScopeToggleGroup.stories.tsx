import type { Meta, StoryObj } from "@storybook/react-vite";
import { ScopeToggleGroup } from "./ScopeToggleGroup";

const meta: Meta<typeof ScopeToggleGroup> = {
  title: "Scenes/Channel Scope/ScopeToggleGroup",
  component: ScopeToggleGroup,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  args: {
    fadersEnabled: true,
    onToggleFaders: () => {},
    onTogglePan: () => {},
    panEnabled: true,
  },
};

export default meta;

type Story = StoryObj<typeof ScopeToggleGroup>;

export const BothEnabled: Story = {};

export const Disabled: Story = {
  args: {
    fadersEnabled: false,
    panEnabled: false,
  },
};

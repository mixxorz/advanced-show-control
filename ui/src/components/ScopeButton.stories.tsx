import type { Meta, StoryObj } from "@storybook/react-vite";
import { ScopeButton } from "./ScopeButton";

const meta: Meta<typeof ScopeButton> = {
  title: "Scenes/Channel Scope/ScopeButton",
  component: ScopeButton,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  args: {
    active: true,
    label: "1",
    onClick: () => {},
    title: "Lead Vocal · -3.5 dB · Pan C",
  },
};

export default meta;

type Story = StoryObj<typeof ScopeButton>;

export const Active: Story = {};

export const Inactive: Story = {
  args: {
    active: false,
  },
};

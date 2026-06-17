import type { Meta, StoryObj } from "@storybook/react-vite";
import { ConsoleButton } from "./ConsoleButton";

const meta: Meta<typeof ConsoleButton> = {
  title: "Primitives/ConsoleButton",
  component: ConsoleButton,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  args: {
    children: "Store",
    onClick: () => {},
  },
};

export default meta;

type Story = StoryObj<typeof ConsoleButton>;

export const Secondary: Story = {};

export const Primary: Story = {
  args: {
    variant: "primary",
  },
};

export const GhostPrimary: Story = {
  args: {
    variant: "ghost-primary",
  },
};

export const GhostSecondary: Story = {
  args: {
    variant: "ghost-secondary",
  },
};

export const Small: Story = {
  args: {
    size: "small",
  },
};

export const SmallGhost: Story = {
  args: {
    size: "small",
    variant: "ghost-primary",
  },
};

export const Active: Story = {
  args: {
    active: true,
    children: "FADER",
  },
};

export const Disabled: Story = {
  args: {
    disabled: true,
  },
};

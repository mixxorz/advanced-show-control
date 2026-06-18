import type { Meta, StoryObj } from "@storybook/react-vite";
import { StatusCell } from "./StatusCell";

const meta: Meta<typeof StatusCell> = {
  title: "Shell/StatusCell",
  component: StatusCell,
  decorators: [
    (Story) => (
      <main className="bg-console-chrome p-6 text-console-primary">
        <div className="max-w-sm border border-console-line">
          <Story />
        </div>
      </main>
    ),
  ],
  args: {
    label: "Current",
    tone: "current",
    value: "S01: The Wonderful Blood",
  },
};

export default meta;

type Story = StoryObj<typeof StatusCell>;

export const Current: Story = {};

export const Warning: Story = {
  args: {
    label: "Mode",
    tone: "warning",
    value: "LOCKOUT",
  },
};

export const Danger: Story = {
  args: {
    label: "Sync",
    tone: "danger",
    value: "Offline",
  },
};

export const Mono: Story = {
  args: {
    font: "mono",
    label: "Time",
    value: "09:41:32 PM",
  },
};

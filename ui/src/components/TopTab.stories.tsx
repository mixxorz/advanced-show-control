import type { Meta, StoryObj } from "@storybook/react-vite";
import { TopTab } from "./TopTab";

const meta: Meta<typeof TopTab> = {
  title: "Shell/TopTab",
  component: TopTab,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  args: {
    active: true,
    children: "Scenes",
    onClick: () => {},
  },
};

export default meta;

type Story = StoryObj<typeof TopTab>;

export const Active: Story = {};

export const Inactive: Story = {
  args: {
    active: false,
  },
};

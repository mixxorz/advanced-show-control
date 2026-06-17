import type { Meta, StoryObj } from "@storybook/react-vite";
import { TopTabBar } from "./TopTabBar";

const meta: Meta<typeof TopTabBar> = {
  title: "Shell/TopTabBar",
  component: TopTabBar,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    activeTab: "scenes",
    onSelectTab: () => {},
  },
};

export default meta;

type Story = StoryObj<typeof TopTabBar>;

export const Scenes: Story = {};

export const Logs: Story = {
  args: {
    activeTab: "logs",
  },
};

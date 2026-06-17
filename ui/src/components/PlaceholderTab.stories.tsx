import type { Meta, StoryObj } from "@storybook/react-vite";
import { PlaceholderTab } from "./PlaceholderTab";

const meta: Meta<typeof PlaceholderTab> = {
  title: "Shell/PlaceholderTab",
  component: PlaceholderTab,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  args: {
    name: "Settings",
  },
};

export default meta;

type Story = StoryObj<typeof PlaceholderTab>;

export const Default: Story = {};

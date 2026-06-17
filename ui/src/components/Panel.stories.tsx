import type { Meta, StoryObj } from "@storybook/react-vite";
import { Panel } from "./Panel";

const meta: Meta<typeof Panel> = {
  title: "Primitives/Panel",
  component: Panel,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  args: {
    children: <div className="p-4">Panel content</div>,
  },
};

export default meta;

type Story = StoryObj<typeof Panel>;

export const Default: Story = {};

export const Padded: Story = {
  args: {
    className: "p-6",
    children: "Padded panel content",
  },
};

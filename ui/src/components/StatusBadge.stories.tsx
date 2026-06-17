import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";
import { StatusBadge } from "./StatusBadge";

const meta = {
  title: "Components/StatusBadge",
  component: StatusBadge,
  parameters: {
    layout: "centered",
  },
} satisfies Meta<typeof StatusBadge>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Neutral: Story = {
  args: {
    label: "Disconnected",
    tone: "neutral",
  },
};

export const Good: Story = {
  args: {
    label: "Connected",
    tone: "good",
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(canvas.getByText("Connected")).toBeInTheDocument();
  },
};

export const Warning: Story = {
  args: {
    label: "Fade: blocked",
    tone: "warning",
  },
};

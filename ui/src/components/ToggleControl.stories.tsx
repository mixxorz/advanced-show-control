import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { ToggleControl } from "./ToggleControl";

const meta = {
  title: "Primitives/ToggleControl",
  component: ToggleControl,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  parameters: { layout: "centered" },
} satisfies Meta<typeof ToggleControl>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    checked: false,
    label: "Toggle primitive",
    onChange: () => {},
  },
  render: () => {
    const [checked, setChecked] = useState(false);
    return (
      <ToggleControl
        checked={checked}
        label="Toggle primitive"
        onChange={setChecked}
      />
    );
  },
};

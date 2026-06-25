import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { StepperControl } from "./StepperControl";

const meta = {
  title: "Primitives/StepperControl",
  component: StepperControl,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  parameters: { layout: "centered" },
} satisfies Meta<typeof StepperControl>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    label: "Stepper primitive",
    max: 10,
    min: 1,
    onChange: () => {},
    value: 7,
  },
  render: () => {
    const [value, setValue] = useState(7);
    return (
      <StepperControl
        label="Stepper primitive"
        max={10}
        min={1}
        onChange={setValue}
        value={value}
      />
    );
  },
};

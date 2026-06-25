import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { StepperControl } from "./StepperControl";

const meta = {
  title: "Primitives/StepperControl",
  component: StepperControl,
  decorators: [
    (Story) => (
      <div className="rounded-console-panel border border-console-line bg-console-chrome p-5">
        <Story />
      </div>
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

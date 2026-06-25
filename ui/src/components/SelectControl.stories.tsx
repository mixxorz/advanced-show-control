import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { SelectControl } from "./SelectControl";

const meta = {
  title: "Primitives/SelectControl",
  component: SelectControl,
  decorators: [
    (Story) => (
      <div className="rounded-console-panel border border-console-line bg-console-chrome p-5">
        <Story />
      </div>
    ),
  ],
  parameters: { layout: "centered" },
} satisfies Meta<typeof SelectControl>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    label: "Select primitive",
    onChange: () => {},
    options: [
      { label: "12 hour", value: "twelveHour" },
      { label: "24 hour", value: "twentyFourHour" },
    ],
    value: "twentyFourHour",
  },
  render: () => {
    const [value, setValue] = useState("twentyFourHour");
    return (
      <SelectControl
        label="Select primitive"
        onChange={setValue}
        options={[
          { label: "12 hour", value: "twelveHour" },
          { label: "24 hour", value: "twentyFourHour" },
        ]}
        value={value}
      />
    );
  },
};

import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { mockAppState } from "../storybook/mockAppState";
import { SettingsTab } from "./SettingsTab";

const meta = {
  title: "Components/SettingsTab",
  component: SettingsTab,
} satisfies Meta<typeof SettingsTab>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <MockAppProviders appState={mockAppState}>
      <SettingsTab />
    </MockAppProviders>
  ),
};

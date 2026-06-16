import type { Meta, StoryObj } from "@storybook/react-vite";
import { LogsTab } from "./LogsTab";
import { connectedAppState } from "../storybook/mockAppState";
import { disconnectedAppViewState } from "../types";

const meta = {
  title: "Components/LogsTab",
  component: LogsTab,
  parameters: {
    layout: "fullscreen",
  },
  decorators: [
    (Story) => (
      <main className="min-h-screen bg-slate-950 p-6 text-slate-100">
        <Story />
      </main>
    ),
  ],
  args: {
    appState: disconnectedAppViewState,
  },
} satisfies Meta<typeof LogsTab>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Empty: Story = {};

export const Populated: Story = {
  args: {
    appState: connectedAppState,
  },
};

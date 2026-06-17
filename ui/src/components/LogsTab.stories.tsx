import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { LogsTab } from "./LogsTab";
import { connectedAppState } from "../storybook/mockAppState";
import { disconnectedAppViewState, type AppViewState } from "../types";

type LogsTabStoryArgs = {
  appState?: AppViewState;
};

const meta: Meta<LogsTabStoryArgs> = {
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
  render: (args) => (
    <MockAppProviders appState={args.appState}>
      <LogsTab />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<LogsTabStoryArgs>;

export const Empty: Story = {};

export const Populated: Story = {
  args: {
    appState: connectedAppState,
  },
};

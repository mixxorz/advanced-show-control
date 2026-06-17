import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { connectedAppState } from "../storybook/mockAppState";
import { disconnectedAppViewState, type AppViewState } from "../types";
import { ConsoleLogsTab } from "./ConsoleLogsTab";

type ConsoleLogsTabStoryArgs = {
  appState?: AppViewState;
};

const meta: Meta<ConsoleLogsTabStoryArgs> = {
  title: "Logs/ConsoleLogsTab",
  component: ConsoleLogsTab,
  parameters: {
    layout: "fullscreen",
  },
  decorators: [
    (Story) => (
      <main className="min-h-screen bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  args: {
    appState: connectedAppState,
  },
  render: (args) => (
    <MockAppProviders appState={args.appState}>
      <ConsoleLogsTab />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<ConsoleLogsTabStoryArgs>;

export const Populated: Story = {};

export const Empty: Story = {
  args: {
    appState: disconnectedAppViewState,
  },
};

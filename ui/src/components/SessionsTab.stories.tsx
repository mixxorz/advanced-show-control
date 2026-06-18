import type { Meta, StoryObj } from "@storybook/react-vite";
import { connectedAppState } from "../storybook/mockAppState";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { SessionsTab } from "./SessionsTab";

const meta: Meta<typeof SessionsTab> = {
  title: "Sessions/SessionsTab",
  component: SessionsTab,
  parameters: {
    layout: "fullscreen",
  },
  decorators: [
    (Story) => (
      <MockAppProviders appState={connectedAppState}>
        <main className="h-screen bg-black p-3 text-console-primary">
          <Story />
        </main>
      </MockAppProviders>
    ),
  ],
};

export default meta;

type Story = StoryObj<typeof SessionsTab>;

export const Default: Story = {};

export const DirtyUntitled: Story = {
  decorators: [
    (Story) => (
      <MockAppProviders
        appState={{
          ...connectedAppState,
          showFileDirty: true,
          showFileName: "Untitled Show",
          showFilePath: null,
        }}
      >
        <main className="h-screen bg-black p-3 text-console-primary">
          <Story />
        </main>
      </MockAppProviders>
    ),
  ],
};

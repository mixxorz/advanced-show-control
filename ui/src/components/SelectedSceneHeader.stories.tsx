import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { connectedAppState, storedVerseScene } from "../storybook/mockAppState";
import { SelectedSceneHeader } from "./SelectedSceneHeader";

const immediateScene = {
  ...storedVerseScene,
  durationMs: 0,
};

const meta: Meta<typeof SelectedSceneHeader> = {
  title: "Scenes/Selected Scene/SelectedSceneHeader",
  component: SelectedSceneHeader,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <MockAppProviders appState={connectedAppState}>
          <Story />
        </MockAppProviders>
      </main>
    ),
  ],
  args: {
    scene: storedVerseScene,
  },
};

export default meta;

type Story = StoryObj<typeof SelectedSceneHeader>;

export const Default: Story = {};

export const Immediate: Story = {
  args: {
    scene: immediateScene,
  },
};

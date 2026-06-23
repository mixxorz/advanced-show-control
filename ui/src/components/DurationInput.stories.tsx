import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { connectedAppState, storedVerseScene } from "../storybook/mockAppState";
import { DurationInput } from "./DurationInput";

const meta: Meta<typeof DurationInput> = {
  title: "Scenes/Selected Scene/DurationInput",
  component: DurationInput,
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
    durationMs: storedVerseScene.durationMs,
    sceneId: storedVerseScene.internalSceneId,
  },
};

export default meta;

type Story = StoryObj<typeof DurationInput>;

export const Default: Story = {};

export const Immediate: Story = {
  args: {
    durationMs: 0,
  },
};

import type { Meta, StoryObj } from "@storybook/react-vite";
import { storedVerseScene } from "../storybook/mockAppState";
import { SelectedSceneTitle } from "./SelectedSceneTitle";

const longNameScene = {
  ...storedVerseScene,
  sceneName: "Very Long Walk-In Music Scene Name That Should Truncate Cleanly",
};

const meta: Meta<typeof SelectedSceneTitle> = {
  title: "Scenes/Selected Scene/SelectedSceneTitle",
  component: SelectedSceneTitle,
  decorators: [
    (Story) => (
      <main className="w-96 bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  args: {
    scene: storedVerseScene,
  },
};

export default meta;

type Story = StoryObj<typeof SelectedSceneTitle>;

export const Default: Story = {};

export const LongName: Story = {
  args: {
    scene: longNameScene,
  },
};

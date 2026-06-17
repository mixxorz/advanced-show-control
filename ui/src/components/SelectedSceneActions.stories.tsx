import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { connectedAppState, storedVerseScene } from "../storybook/mockAppState";
import { SelectedSceneActions } from "./SelectedSceneActions";

const meta: Meta<typeof SelectedSceneActions> = {
  title: "Scenes/Selected Scene/SelectedSceneActions",
  component: SelectedSceneActions,
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
    sceneId: storedVerseScene.sceneId,
  },
};

export default meta;

type Story = StoryObj<typeof SelectedSceneActions>;

export const Default: Story = {};

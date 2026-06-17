import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { connectedAppState } from "../storybook/mockAppState";
import { disconnectedAppViewState, type AppViewState } from "../types";
import { SceneEditor } from "./SceneEditor";

type SceneEditorStoryArgs = {
  appState?: AppViewState;
};

const meta: Meta<SceneEditorStoryArgs> = {
  title: "Scenes/SceneEditor",
  component: SceneEditor,
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
      <SceneEditor />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<SceneEditorStoryArgs>;

export const Selected: Story = {};

export const Empty: Story = {
  args: {
    appState: disconnectedAppViewState,
  },
};

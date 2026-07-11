import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import {
  connectedAppState,
  storedVerseScene,
  unlinkedDraftScene,
} from "../storybook/mockAppState";
import type { AppViewState, SceneConfig } from "../types";
import { SelectedSceneActions } from "./SelectedSceneActions";

type SelectedSceneActionsStoryArgs = {
  appState: AppViewState;
  scene: SceneConfig;
};

const meta: Meta<SelectedSceneActionsStoryArgs> = {
  title: "Scenes/Selected Scene/SelectedSceneActions",
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  args: {
    appState: connectedAppState,
    scene: storedVerseScene,
  },
  render: (args) => (
    <MockAppProviders appState={args.appState}>
      <SelectedSceneActions scene={args.scene} />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<SelectedSceneActionsStoryArgs>;

export const ClipboardUnavailable: Story = {
  args: {
    appState: {
      ...connectedAppState,
      sceneSettingsClipboardAvailable: false,
    },
  },
};

export const ClipboardAvailable: Story = {
  args: {
    appState: {
      ...connectedAppState,
      sceneSettingsClipboardAvailable: true,
    },
  },
};

export const UnlinkedDestination: Story = {
  args: {
    appState: {
      ...connectedAppState,
      sceneSettingsClipboardAvailable: true,
    },
    scene: unlinkedDraftScene,
  },
};

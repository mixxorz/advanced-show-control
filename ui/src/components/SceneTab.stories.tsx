import type { Meta, StoryObj } from "@storybook/react-vite";
import { SceneTab } from "./SceneTab";
import {
  connectedAppState,
  connectedWithDuplicateScenesAppState,
  storedChorusScene,
} from "../storybook/mockAppState";
import { disconnectedAppViewState } from "../types";

const promiseTrue = async () => true;
const noop = () => {};

const meta = {
  title: "Components/SceneTab",
  component: SceneTab,
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
    appState: connectedAppState,
    selectScene: noop,
    setSceneDurationMs: promiseTrue,
    setSceneScopeFadersEnabled: noop,
    setSceneScopePanEnabled: noop,
    storeSceneConfig: promiseTrue,
    setChannelScoped: noop,
    setAllChannelsScoped: noop,
  },
} satisfies Meta<typeof SceneTab>;

export default meta;

type Story = StoryObj<typeof meta>;

export const StoredSceneSelected: Story = {};

export const DuplicateSceneWarning: Story = {
  args: {
    appState: connectedWithDuplicateScenesAppState,
  },
};

export const ChorusSelected: Story = {
  args: {
    appState: {
      ...connectedAppState,
      selectedSceneId: storedChorusScene.sceneId,
    },
  },
};

export const NoScenes: Story = {
  args: {
    appState: disconnectedAppViewState,
  },
};

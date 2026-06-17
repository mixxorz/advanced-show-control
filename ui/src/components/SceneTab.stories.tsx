import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { SceneTab } from "./SceneTab";
import {
  connectedAppState,
  connectedWithDuplicateScenesAppState,
  storedChorusScene,
} from "../storybook/mockAppState";
import { disconnectedAppViewState } from "../types";

const meta: Meta<any> = {
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
  },
  render: (args: any) => (
    <MockAppProviders appState={args.appState}>
      <SceneTab />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<any>;

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

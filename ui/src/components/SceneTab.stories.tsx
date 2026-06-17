import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { SceneTab } from "./SceneTab";
import {
  connectedAppState,
  connectedWithDuplicateScenesAppState,
  storedChorusScene,
} from "../storybook/mockAppState";
import { disconnectedAppViewState, type AppViewState } from "../types";

type SceneTabStoryArgs = {
  appState?: AppViewState;
};

const meta: Meta<SceneTabStoryArgs> = {
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
  render: (args) => (
    <MockAppProviders appState={args.appState}>
      <SceneTab />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<SceneTabStoryArgs>;

export const StoredSceneSelected: Story = {};

StoredSceneSelected.play = async ({ canvasElement }) => {
  const canvas = within(canvasElement);

  await expect(
    canvas.getByRole("heading", { name: "Scenes" }),
  ).toBeInTheDocument();
  await expect(
    canvas.getByRole("heading", { name: "4: Verse" }),
  ).toBeInTheDocument();
};

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

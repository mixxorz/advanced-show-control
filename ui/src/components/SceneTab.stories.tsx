import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, within } from "storybook/test";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { SceneTab } from "./SceneTab";
import {
  connectedAppState,
  connectedWithDuplicateScenesAppState,
  storedChorusScene,
  unlinkedDraftScene,
} from "../storybook/mockAppState";
import { disconnectedAppViewState, type AppViewState } from "../types";

type SceneTabStoryArgs = {
  appState?: AppViewState;
};

const meta: Meta<SceneTabStoryArgs> = {
  title: "Scenes/SceneTab",
  component: SceneTab,
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
      <SceneTab />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<SceneTabStoryArgs>;

const unlinkedAlternateScene = {
  ...unlinkedDraftScene,
  internalSceneId: "scene-alternate-unlinked",
  sceneName: "Deleted Alternate Scene",
  durationMs: 3500,
};

const linkSceneControlsAppState = {
  ...connectedAppState,
  sceneConfigs: [
    { ...connectedAppState.sceneConfigs[0], sceneIndex: 0 },
    ...connectedAppState.sceneConfigs.slice(1),
    unlinkedDraftScene,
    unlinkedAlternateScene,
  ],
  selectedSceneInternalId: unlinkedDraftScene.internalSceneId,
};

export const StoredSceneSelected: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(
      canvas.getByRole("heading", { name: "Scene List" }),
    ).toBeInTheDocument();
    await expect(
      canvas.getByRole("button", { name: "Recall" }),
    ).toBeInTheDocument();
  },
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
      selectedSceneInternalId: storedChorusScene.internalSceneId,
    },
  },
};

export const LinkSceneControls: Story = {
  args: {
    appState: linkSceneControlsAppState,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(
      canvas.getByRole("button", { name: "Link to scene" }),
    ).toBeInTheDocument();
    await expect(
      canvas.getByRole("button", { name: "Delete" }),
    ).toBeInTheDocument();
  },
};

export const LinkSceneOverwriteModal: Story = {
  args: LinkSceneControls.args,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await userEvent.selectOptions(canvas.getByLabelText("LV1 Scene"), "0");
    await userEvent.click(
      canvas.getByRole("button", { name: "Link to scene" }),
    );

    await expect(canvas.getByRole("dialog")).toHaveTextContent(
      "Overwrite Existing Fade Settings?",
    );
    await expect(
      canvas.getByRole("button", { name: "Overwrite" }),
    ).toBeInTheDocument();
  },
};

export const NoScenes: Story = {
  args: {
    appState: disconnectedAppViewState,
  },
};

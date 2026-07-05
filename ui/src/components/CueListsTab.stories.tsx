import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, within } from "storybook/test";
import { MockAppProviders } from "../storybook/MockAppProviders";
import {
  connectedAppState,
  cueListWithMissingSceneReferenceAppState,
} from "../storybook/mockAppState";
import type { AppViewState } from "../types";
import { CueListsTab } from "./CueListsTab";

type CueListsTabStoryArgs = {
  appState?: AppViewState;
};

const duplicateCueListAppState: AppViewState = {
  ...connectedAppState,
  cueLists: [
    {
      id: "cue-list-main",
      name: "Main",
      entries: [
        { id: "cue-1", sceneInternalId: "scene-verse" },
        { id: "cue-2", sceneInternalId: "scene-chorus" },
        { id: "cue-3", sceneInternalId: "scene-verse" },
      ],
    },
  ],
  activeCueListId: "cue-list-main",
  cuedCueEntryId: "cue-2",
  lastCueRecallStatus: "recalling next cue",
};

const emptyCueListsAppState: AppViewState = {
  ...connectedAppState,
  cueLists: [],
  activeCueListId: null,
  cuedCueEntryId: null,
  lastCueRecallStatus: null,
};

const meta: Meta<CueListsTabStoryArgs> = {
  title: "Cue Lists/CueListsTab",
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
      <CueListsTab />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<CueListsTabStoryArgs>;

export const Empty: Story = {
  args: {
    appState: emptyCueListsAppState,
  },
};

export const ActiveListWithDuplicates: Story = {
  args: {
    appState: duplicateCueListAppState,
  },
};

export const MissingSceneReference: Story = {
  args: {
    appState: cueListWithMissingSceneReferenceAppState,
  },
};

export const ManageModalOpen: Story = {
  args: {
    appState: connectedAppState,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await userEvent.click(
      canvas.getByRole("button", { name: "Manage Cue Lists" }),
    );

    const manageDialog = canvas.getByRole("dialog", {
      name: "Manage Cue Lists",
    });

    await expect(manageDialog).toBeInTheDocument();
    await expect(
      within(manageDialog).getAllByText("Main").length,
    ).toBeGreaterThan(0);
    await expect(
      within(manageDialog).getAllByText("Verse").length,
    ).toBeGreaterThan(0);
  },
};

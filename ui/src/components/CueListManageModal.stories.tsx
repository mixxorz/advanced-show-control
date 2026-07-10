import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, within } from "storybook/test";
import { MockAppProviders } from "../storybook/MockAppProviders";
import {
  connectedAppState,
  cueListManageModalAppState,
} from "../storybook/mockAppState";
import type { AppViewState } from "../types";
import { CueListManageModal } from "./CueListManageModal";

type CueListManageModalStoryArgs = {
  appState?: AppViewState;
};

const emptyCueListAppState: AppViewState = {
  ...connectedAppState,
  cueLists: [],
  activeCueListId: null,
};

const meta: Meta<CueListManageModalStoryArgs> = {
  title: "Cue Lists/CueListManageModal",
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
    appState: cueListManageModalAppState,
  },
  render: (args) => (
    <MockAppProviders appState={args.appState}>
      <CueListManageModal onClose={() => {}} />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<CueListManageModalStoryArgs>;

export const MultipleCueLists: Story = {};

export const Empty: Story = {
  args: {
    appState: emptyCueListAppState,
  },
};

export const DeleteConfirmationOpen: Story = {
  args: {
    appState: cueListManageModalAppState,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await userEvent.click(
      canvas.getByRole("button", { name: "Delete Mid Set" }),
    );

    await expect(
      canvas.getByRole("dialog", { name: "Delete Cue List" }),
    ).toHaveTextContent(
      "Delete Mid Set? This only removes the app-managed cue list.",
    );
  },
};

import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";
import { MockAppProviders } from "../storybook/MockAppProviders";
import {
  connectedWithUnlinkedSceneAppState,
  unlinkedDraftScene,
} from "../storybook/mockAppState";
import { LinkSceneControls } from "./LinkSceneControls";

const meta: Meta<typeof LinkSceneControls> = {
  title: "Scenes/LinkSceneControls",
  component: LinkSceneControls,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  args: {
    existingConfigs: connectedWithUnlinkedSceneAppState.sceneConfigs,
    lv1Scenes: connectedWithUnlinkedSceneAppState.scenes,
    scene: unlinkedDraftScene,
  },
  render: (args) => (
    <MockAppProviders appState={connectedWithUnlinkedSceneAppState}>
      <LinkSceneControls {...args} />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<typeof LinkSceneControls>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(
      canvas.getByText("Scene is currently unlinked"),
    ).toBeInTheDocument();
    await expect(
      canvas.getByRole("button", { name: "Link to scene" }),
    ).toBeInTheDocument();
    await expect(
      canvas.getByRole("button", { name: "Delete" }),
    ).toBeInTheDocument();
  },
};

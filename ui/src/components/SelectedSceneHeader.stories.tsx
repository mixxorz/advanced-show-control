import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { connectedAppState, storedVerseScene } from "../storybook/mockAppState";
import { SelectedSceneHeader } from "./SelectedSceneHeader";

const immediateScene = {
  ...storedVerseScene,
  durationMs: 0,
};

const meta: Meta<typeof SelectedSceneHeader> = {
  title: "Scenes/Selected Scene/SelectedSceneHeader",
  component: SelectedSceneHeader,
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
    currentScene: null,
    cued: false,
    scene: storedVerseScene,
  },
};

export default meta;

type Story = StoryObj<typeof SelectedSceneHeader>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(canvas.getByLabelText("Selected scene")).toHaveTextContent(
      "004 S01: The Wonderful Blood",
    );
    await expect(canvas.getByRole("button", { name: "Store" })).toHaveClass(
      "text-[1.1rem]",
    );
  },
};

export const Immediate: Story = {
  args: {
    scene: immediateScene,
  },
};

import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";
import { OverwriteSceneLinkModal } from "./OverwriteSceneLinkModal";

const meta: Meta<typeof OverwriteSceneLinkModal> = {
  title: "Scenes/OverwriteSceneLinkModal",
  component: OverwriteSceneLinkModal,
  decorators: [
    (Story) => (
      <main className="min-h-screen bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  args: {
    onCancel: () => {},
    onOverwrite: () => {},
    sourceSceneName: "Deleted Draft Scene",
    targetSceneIndex: 0,
    targetSceneName: "Service Start",
  },
};

export default meta;

type Story = StoryObj<typeof OverwriteSceneLinkModal>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(canvas.getByRole("dialog")).toHaveTextContent(
      "Overwrite Existing Fade Settings?",
    );
    await expect(canvas.getByRole("dialog")).toHaveTextContent(
      "001 Service Start already has fade settings. If you continue, those settings will be replaced with the fade settings from Deleted Draft Scene.",
    );
    await expect(canvas.getByRole("dialog")).toHaveTextContent(
      "This only changes the fade settings saved in Advanced Show Control. No changes are made to the actual scene in the console.",
    );
    await expect(
      canvas.getByRole("button", { name: "Cancel" }),
    ).toBeInTheDocument();
    await expect(
      canvas.getByRole("button", { name: "Overwrite" }),
    ).toBeInTheDocument();
  },
};

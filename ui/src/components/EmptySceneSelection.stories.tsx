import type { Meta, StoryObj } from "@storybook/react-vite";
import { EmptySceneSelection } from "./EmptySceneSelection";

const meta: Meta<typeof EmptySceneSelection> = {
  title: "Scenes/EmptySceneSelection",
  component: EmptySceneSelection,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
};

export default meta;

type Story = StoryObj<typeof EmptySceneSelection>;

export const Default: Story = {};

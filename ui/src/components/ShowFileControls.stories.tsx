import type { Meta, StoryObj } from "@storybook/react-vite";
import { ShowFileControls } from "./ShowFileControls";

const meta: Meta<typeof ShowFileControls> = {
  title: "Show Files/ShowFileControls",
  component: ShowFileControls,
  decorators: [
    (Story) => (
      <main className="bg-slate-950 p-6 text-slate-100">
        <Story />
      </main>
    ),
  ],
  args: {
    dirty: false,
    fileName: "Sunday Service.ascshow",
    filePath: "/Users/engineer/Shows/Sunday Service.ascshow",
    onNew: () => {},
    onOpen: () => {},
    onSave: () => {},
    onSaveAs: () => {},
  },
};

export default meta;

type Story = StoryObj<typeof ShowFileControls>;

export const Saved: Story = {};

export const DirtyUntitled: Story = {
  args: {
    dirty: true,
    fileName: "Untitled Show",
    filePath: null,
  },
};

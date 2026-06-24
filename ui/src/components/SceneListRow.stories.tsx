import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  storedVerseScene,
  unlinkedDraftScene,
} from "../storybook/mockAppState";
import { SceneListRow } from "./SceneListRow";

const meta: Meta<typeof SceneListRow> = {
  title: "Scenes/Scene List/SceneListRow",
  component: SceneListRow,
  decorators: [
    (Story) => (
      <main className="w-96 bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  args: {
    currentScene: null,
    cued: false,
    onSelect: () => {},
    scene: storedVerseScene,
    selected: false,
  },
};

export default meta;

type Story = StoryObj<typeof SceneListRow>;

export const Idle: Story = {};

export const Current: Story = {
  args: {
    currentScene: {
      index: storedVerseScene.sceneIndex ?? 0,
      name: storedVerseScene.sceneName,
    },
  },
};

export const Cued: Story = {
  args: {
    cued: true,
  },
};

export const Unlinked: Story = {
  args: {
    scene: unlinkedDraftScene,
  },
};

export const IdleSelected: Story = {
  args: {
    selected: true,
  },
};

export const CurrentSelected: Story = {
  args: {
    currentScene: {
      index: storedVerseScene.sceneIndex ?? 0,
      name: storedVerseScene.sceneName,
    },
    selected: true,
  },
};

export const CuedSelected: Story = {
  args: {
    cued: true,
    selected: true,
  },
};

export const UnlinkedSelected: Story = {
  args: {
    scene: unlinkedDraftScene,
    selected: true,
  },
};

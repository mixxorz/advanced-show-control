import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  connectedAppState,
  discoveringAppState,
} from "../storybook/mockAppState";
import { BottomStatusBar } from "./BottomStatusBar";

const lockedOutAppState = {
  ...connectedAppState,
  lockout: true,
};

const meta: Meta<typeof BottomStatusBar> = {
  title: "Shell/BottomStatusBar",
  component: BottomStatusBar,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    appState: connectedAppState,
  },
};

export default meta;

type Story = StoryObj<typeof BottomStatusBar>;

export const Connected: Story = {};

export const Lockout: Story = {
  args: {
    appState: lockedOutAppState,
  },
};

export const Offline: Story = {
  args: {
    appState: discoveringAppState,
  },
};

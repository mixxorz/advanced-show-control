import type { Meta, StoryObj } from "@storybook/react-vite";
import { Header } from "./Header";
import {
  connectedAppState,
  discoveredSystemsAppState,
  discoveringAppState,
} from "../storybook/mockAppState";

const noop = () => {};

const meta = {
  title: "Components/Header",
  component: Header,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    appState: connectedAppState,
    commandError: null,
    onAbortAll: noop,
    onNewShowFile: noop,
    onOpenConnection: noop,
    onOpenShowFile: noop,
    onSaveShowFile: noop,
    onSaveShowFileAs: noop,
    onToggleLockout: noop,
  },
} satisfies Meta<typeof Header>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Connected: Story = {};

export const LockoutRunningFade: Story = {
  args: {
    appState: {
      ...connectedAppState,
      lockout: true,
      fadeState: "running",
    },
  },
};

export const CommandError: Story = {
  args: {
    appState: discoveringAppState,
    commandError: "Permission denied: LV1 rejected the command.",
  },
};

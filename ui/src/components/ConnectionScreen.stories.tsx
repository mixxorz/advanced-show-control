import type { Meta, StoryObj } from "@storybook/react-vite";
import { ConnectionScreen } from "./ConnectionScreen";
import {
  connectedAppState,
  discoveredSystemsAppState,
  discoveringAppState,
} from "../storybook/mockAppState";

const noop = async () => {};

const meta = {
  title: "Components/ConnectionScreen",
  component: ConnectionScreen,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    appState: discoveringAppState,
    commandError: null,
    onDisconnect: noop,
    onSelectSystem: noop,
    onResume: noop,
  },
} satisfies Meta<typeof ConnectionScreen>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Searching: Story = {};

export const SystemsFound: Story = {
  args: {
    appState: discoveredSystemsAppState,
  },
};

export const Connected: Story = {
  args: {
    appState: connectedAppState,
  },
};

export const CommandError: Story = {
  args: {
    appState: discoveredSystemsAppState,
    commandError: "Unable to connect to LV1 system.",
  },
};

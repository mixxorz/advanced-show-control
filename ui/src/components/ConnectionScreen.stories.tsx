import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { ConnectionScreen } from "./ConnectionScreen";
import {
  connectedAppState,
  discoveredSystemsAppState,
} from "../storybook/mockAppState";
import type { AppViewState } from "../types";

const noop = async () => {};

type ConnectionScreenStoryArgs = ComponentProps<typeof ConnectionScreen> & {
  appState?: AppViewState;
  commandError?: string | null;
};

const meta: Meta<ConnectionScreenStoryArgs> = {
  title: "Connection/ConnectionScreen",
  component: ConnectionScreen,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    onResume: noop,
  },
  render: (args) => (
    <MockAppProviders appState={args.appState} commandError={args.commandError}>
      <ConnectionScreen onResume={args.onResume} />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<ConnectionScreenStoryArgs>;

export const Searching: Story = {};

export const SystemsFound: Story = {
  args: {
    appState: discoveredSystemsAppState,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(
      canvas.getByRole("heading", { name: "Choose an LV1 system" }),
    ).toBeInTheDocument();
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

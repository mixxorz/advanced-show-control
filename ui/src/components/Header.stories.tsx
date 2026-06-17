import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { Header } from "./Header";
import { connectedAppState } from "../storybook/mockAppState";
import type { AppViewState } from "../types";

const noop = () => {};

type HeaderStoryArgs = ComponentProps<typeof Header> & {
  appState?: AppViewState;
  commandError?: string | null;
};

const meta: Meta<HeaderStoryArgs> = {
  title: "Shell/Header",
  component: Header,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    onOpenConnection: noop,
  },
  render: (args) => (
    <MockAppProviders appState={args.appState} commandError={args.commandError}>
      <Header onOpenConnection={args.onOpenConnection} />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<HeaderStoryArgs>;

export const Connected: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(
      canvas.getByRole("heading", { name: "Advanced Show Control" }),
    ).toBeInTheDocument();
    await expect(
      canvas.getByRole("button", { name: "Open LV1 connection screen" }),
    ).toBeInTheDocument();
  },
};

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
    commandError: "Permission denied: LV1 rejected the command.",
  },
};

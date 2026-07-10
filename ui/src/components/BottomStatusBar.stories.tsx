import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  connectedAppState,
  discoveringAppState,
  cueListNoValidCueAppState,
} from "../storybook/mockAppState";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { BottomStatusBar } from "./BottomStatusBar";

const safeAppState = {
  ...connectedAppState,
  lockout: true,
};

const fadingAppState = {
  ...connectedAppState,
  fadeState: "running" as const,
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
  render: (args) => (
    <MockAppProviders appState={args.appState}>
      <BottomStatusBar {...args} />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<typeof BottomStatusBar>;

export const Ready: Story = {};

export const NoValidCue: Story = {
  args: {
    appState: cueListNoValidCueAppState,
  },
};

export const Safe: Story = {
  args: {
    appState: safeAppState,
  },
};

export const Fading: Story = {
  args: {
    appState: fadingAppState,
  },
};

export const Offline: Story = {
  args: {
    appState: discoveringAppState,
  },
};

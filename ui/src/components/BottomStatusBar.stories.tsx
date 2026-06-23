import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  connectedAppState,
  discoveringAppState,
} from "../storybook/mockAppState";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { BottomStatusBar } from "./BottomStatusBar";

const cuedConnectedAppState = {
  ...connectedAppState,
  cuedSceneInternalId:
    connectedAppState.sceneConfigs[1]?.internalSceneId ?? null,
};

const safeAppState = {
  ...cuedConnectedAppState,
  lockout: true,
};

const fadingAppState = {
  ...cuedConnectedAppState,
  fadeState: "running" as const,
};

const meta: Meta<typeof BottomStatusBar> = {
  title: "Shell/BottomStatusBar",
  component: BottomStatusBar,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    appState: cuedConnectedAppState,
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

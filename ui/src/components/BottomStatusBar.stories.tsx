import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  connectedAppState,
  discoveringAppState,
} from "../storybook/mockAppState";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { BottomStatusBar } from "./BottomStatusBar";

const cuedConnectedAppState = {
  ...connectedAppState,
  cuedSceneId: connectedAppState.sceneConfigs[1]?.sceneId ?? null,
};

const lockedOutAppState = {
  ...cuedConnectedAppState,
  lockout: true,
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

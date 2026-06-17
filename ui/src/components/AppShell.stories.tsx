import type { Meta, StoryObj } from "@storybook/react-vite";
import { disconnectedAppViewState } from "../types";
import {
  connectedAppState,
  discoveredSystemsAppState,
  discoveringAppState,
} from "../storybook/mockAppState";
import { AppShell } from "./AppShell";

const noop = () => {};
const promiseTrue = async () => true;

const meta = {
  title: "App/AppShell",
  component: AppShell,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    activeTab: "scene",
    appState: connectedAppState,
    commandError: null,
    onAbortAll: noop,
    onDisconnect: promiseTrue,
    onNewShowFile: noop,
    onOpenConnection: noop,
    onOpenShowFile: noop,
    onResume: noop,
    onSaveShowFile: noop,
    onSaveShowFileAs: noop,
    onSelectScene: noop,
    onSelectSystem: noop,
    onSelectTab: noop,
    onSetAllChannelsScoped: noop,
    onSetChannelScoped: noop,
    onSetSceneDurationMs: promiseTrue,
    onSetSceneScopeFadersEnabled: noop,
    onSetSceneScopePanEnabled: noop,
    onStoreSceneConfig: promiseTrue,
    onToggleLockout: noop,
    showConnection: false,
  },
} satisfies Meta<typeof AppShell>;

export default meta;

type Story = StoryObj<typeof meta>;

export const ConnectionSearching: Story = {
  args: {
    appState: discoveringAppState,
    showConnection: true,
  },
};

export const ConnectionSystemsFound: Story = {
  args: {
    appState: discoveredSystemsAppState,
    showConnection: true,
  },
};

export const SceneTab: Story = {};

export const LogsTab: Story = {
  args: {
    activeTab: "logs",
  },
};

export const CommandError: Story = {
  args: {
    commandError: "Unable to save show file: permission denied.",
  },
};

export const ReconnectOverlay: Story = {
  args: {
    appState: {
      ...connectedAppState,
      reconnect: { active: true, attempt: 2 },
    },
  },
};

export const EmptyMainShell: Story = {
  args: {
    appState: disconnectedAppViewState,
  },
};

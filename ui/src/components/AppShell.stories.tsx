import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";
import { disconnectedAppViewState } from "../types";
import {
  connectedAppState,
  discoveredSystemsAppState,
  discoveringAppState,
} from "../storybook/mockAppState";
import { MockAppProviders } from "../storybook/MockAppProviders";
import type { AppViewState } from "../types";
import { AppShell } from "./AppShell";

type AppShellStoryArgs = ComponentProps<typeof AppShell> & {
  appState?: AppViewState;
  commandError?: string | null;
};

const meta: Meta<AppShellStoryArgs> = {
  title: "App/AppShell",
  component: AppShell,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    activeTab: "scenes",
    onOpenConnection: () => {},
    onResume: () => {},
    onSelectTab: () => {},
    showConnection: false,
  },
  render: (args) => (
    <MockAppProviders appState={args.appState} commandError={args.commandError}>
      <AppShell
        activeTab={args.activeTab}
        onOpenConnection={args.onOpenConnection}
        onResume={args.onResume}
        onSelectTab={args.onSelectTab}
        showConnection={args.showConnection}
      />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<AppShellStoryArgs>;

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

export const SceneTab: Story = {
  args: {
    appState: connectedAppState,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(
      canvas.getByRole("heading", { name: "Scene List" }),
    ).toBeInTheDocument();
    await expect(
      canvas.getByRole("button", { name: "Scenes" }),
    ).toBeInTheDocument();
    await expect(
      canvas.getByRole("button", { name: "Settings" }),
    ).toBeInTheDocument();
  },
};

export const LogsTab: Story = {
  args: {
    activeTab: "logs",
  },
};

export const SettingsPlaceholder: Story = {
  args: {
    activeTab: "settings",
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

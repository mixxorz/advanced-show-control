import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { ConnectionModal } from "./ConnectionModal";
import {
  connectedAppState,
  discoveredSystemsAppState,
} from "../storybook/mockAppState";
import type { AppViewState, DiscoveredLv1System } from "../types";

const noop = async () => {};
const disconnect = fn();

const manyDiscoveredSystems: DiscoveredLv1System[] = Array.from(
  { length: 14 },
  (_, index) => ({
    identity: {
      uuid: `lv1-demo-${index}`,
      host: index % 3 === 0 ? null : `FOH LV1 ${index + 1}`,
      address: `192.168.1.${42 + index}`,
      port: 22000,
    },
    status: index % 4 === 0 ? "unavailable" : "available",
  }),
);

const manySystemsAppState: AppViewState = {
  ...discoveredSystemsAppState,
  discoveredLv1Systems: manyDiscoveredSystems,
};

const connectedSystemsAppState: AppViewState = {
  ...connectedAppState,
  discoveredLv1Systems: [
    {
      identity: {
        uuid: "lv1-demo",
        host: "FOH LV1",
        address: "192.168.1.42",
        port: 22000,
      },
      status: "connected",
    },
    {
      identity: {
        uuid: "lv1-monitor",
        host: "Monitor LV1",
        address: "192.168.1.43",
        port: 22000,
      },
      status: "available",
    },
    {
      identity: {
        uuid: "lv1-broadcast",
        host: "Broadcast LV1",
        address: "192.168.1.44",
        port: 22000,
      },
      status: "unavailable",
    },
  ],
};

type ConnectionModalStoryArgs = ComponentProps<typeof ConnectionModal> & {
  appState?: AppViewState;
  commandError?: string | null;
};

const meta: Meta<ConnectionModalStoryArgs> = {
  title: "Connection/ConnectionModal",
  component: ConnectionModal,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    onResume: noop,
  },
  render: (args) => (
    <MockAppProviders
      appState={args.appState}
      commandError={args.commandError}
      commands={{ disconnect }}
    >
      <div className="h-screen bg-black">
        <ConnectionModal onResume={args.onResume} />
      </div>
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<ConnectionModalStoryArgs>;

export const Searching: Story = {};

export const SystemsFound: Story = {
  args: {
    appState: discoveredSystemsAppState,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(
      canvas.getByRole("heading", { name: "Connect to LV1" }),
    ).toBeInTheDocument();
  },
};

export const ManySystems: Story = {
  args: {
    appState: manySystemsAppState,
  },
};

export const Connected: Story = {
  args: {
    appState: connectedSystemsAppState,
  },
  play: async ({ canvasElement }) => {
    disconnect.mockClear();
    const canvas = within(canvasElement);

    await userEvent.click(canvas.getByRole("button", { name: "Disconnect" }));

    await expect(disconnect).toHaveBeenCalledTimes(1);
    await expect(
      canvas.getByRole("heading", { name: "Connect to LV1" }),
    ).toBeInTheDocument();
  },
};

export const CommandError: Story = {
  args: {
    appState: discoveredSystemsAppState,
    commandError: "Unable to connect to LV1 system.",
  },
};

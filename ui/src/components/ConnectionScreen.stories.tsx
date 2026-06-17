import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { ConnectionScreen } from "./ConnectionScreen";
import { connectedAppState, discoveredSystemsAppState, discoveringAppState } from "../storybook/mockAppState";

const noop = async () => {};

const meta: Meta<any> = {
  title: "Components/ConnectionScreen",
  component: ConnectionScreen,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    onResume: noop,
  },
  render: (args: any) => (
    <MockAppProviders appState={args.appState} commandError={args.commandError}>
      <ConnectionScreen onResume={args.onResume} />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<any>;

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

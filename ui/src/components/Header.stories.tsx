import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { Header } from "./Header";
import { connectedAppState } from "../storybook/mockAppState";

const noop = () => {};

const meta: Meta<any> = {
  title: "Components/Header",
  component: Header,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    onOpenConnection: noop,
  },
  render: (args: any) => (
    <MockAppProviders appState={args.appState} commandError={args.commandError}>
      <Header onOpenConnection={args.onOpenConnection} />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<any>;

export const Connected: Story = {};

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

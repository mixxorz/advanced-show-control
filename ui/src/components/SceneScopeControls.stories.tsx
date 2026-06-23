import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { connectedAppState, storedVerseScene } from "../storybook/mockAppState";
import { SceneScopeControls } from "./SceneScopeControls";

const meta: Meta<typeof SceneScopeControls> = {
  title: "Scenes/Selected Scene/SceneScopeControls",
  component: SceneScopeControls,
  decorators: [
    (Story) => (
      <main className="bg-console-bg p-6 text-console-primary">
        <MockAppProviders appState={connectedAppState}>
          <Story />
        </MockAppProviders>
      </main>
    ),
  ],
  args: {
    sceneId: storedVerseScene.internalSceneId,
    scopeToggles: { faders: true, pan: true },
  },
};

export default meta;

type Story = StoryObj<typeof SceneScopeControls>;

export const BothEnabled: Story = {};

export const FadersOnly: Story = {
  args: {
    scopeToggles: { faders: true, pan: false },
  },
};

export const Disabled: Story = {
  args: {
    scopeToggles: { faders: false, pan: false },
  },
};

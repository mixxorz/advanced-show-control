import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { mockAppState } from "../storybook/mockAppState";
import type { AppSettings, AppViewState } from "../types";
import { SettingsTab } from "./SettingsTab";

const meta = {
  title: "Settings/SettingsTab",
  component: SettingsTab,
} satisfies Meta<typeof SettingsTab>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <InteractiveSettingsTab />,
};

function InteractiveSettingsTab() {
  const [appState, setAppState] = useState<AppViewState>(mockAppState);

  function replaceSettings(settings: AppSettings) {
    setAppState((state) => ({ ...state, settings }));
  }

  return (
    <MockAppProviders appState={appState}>
      <SettingsTab onReplaceSettings={replaceSettings} />
    </MockAppProviders>
  );
}

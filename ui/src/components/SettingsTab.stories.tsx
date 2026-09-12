import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { userEvent, within } from "storybook/test";
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
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByLabelText("Same scene recall finishing"));
    await userEvent.click(
      canvas.getByRole("button", {
        name: "Increase Same scene recall threshold",
      }),
    );
  },
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

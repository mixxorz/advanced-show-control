import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, within } from "storybook/test";
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
    const finishing = canvas.getByLabelText("Same scene recall finishing");
    const threshold = canvas.getByLabelText("Same scene recall threshold");

    await expect(finishing).toHaveAttribute("aria-pressed", "true");
    await expect(threshold).toHaveValue("500 ms");
    await userEvent.click(finishing);
    await expect(finishing).toHaveAttribute("aria-pressed", "false");
    await userEvent.click(
      canvas.getByRole("button", {
        name: "Increase Same scene recall threshold",
      }),
    );
    await expect(threshold).toHaveValue("600 ms");
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

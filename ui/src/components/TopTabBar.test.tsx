import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { connectedAppState } from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import { disconnectedAppViewState, type AppViewState } from "../types";
import { TopTabBar } from "./TopTabBar";

function renderTopBar(appState: AppViewState, onOpenConnection = vi.fn()) {
  renderWithAppProviders(
    <TopTabBar
      activeTab="scenes"
      onOpenConnection={onOpenConnection}
      onSelectTab={vi.fn()}
    />,
    { appState },
  );

  return { onOpenConnection };
}

describe("TopTabBar", () => {
  it("shows connected status and console name", () => {
    renderTopBar(connectedAppState);

    expect(screen.getByText("Connected")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /FOH LV1/i }),
    ).toBeInTheDocument();
  });

  it("does not report connected while disconnected or connecting", () => {
    renderTopBar(disconnectedAppViewState);

    expect(screen.getByText("Offline")).toBeInTheDocument();
    expect(screen.queryByText("Connected")).not.toBeInTheDocument();

    renderTopBar({ ...disconnectedAppViewState, connection: "connecting" });

    expect(screen.getByText("Connecting")).toBeInTheDocument();
    expect(screen.queryByText("Connected")).not.toBeInTheDocument();
  });

  it("opens the connection modal from the console button", async () => {
    const user = userEvent.setup();
    const { onOpenConnection } = renderTopBar(connectedAppState);

    await user.click(screen.getByRole("button", { name: /FOH LV1/i }));

    expect(onOpenConnection).toHaveBeenCalledTimes(1);
  });
});

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

  it("renders a fixed-label SAFE button", () => {
    renderTopBar(connectedAppState);

    expect(screen.getByRole("button", { name: "SAFE" })).toBeInTheDocument();
  });

  it("does not render a Sessions tab", () => {
    renderTopBar(connectedAppState);

    expect(
      screen.queryByRole("button", { name: "Sessions" }),
    ).not.toBeInTheDocument();
  });

  it("toggles lockout from the SAFE button", async () => {
    const user = userEvent.setup();
    const toggleLockout = vi.fn();

    renderWithAppProviders(
      <TopTabBar
        activeTab="scenes"
        onOpenConnection={vi.fn()}
        onSelectTab={vi.fn()}
      />,
      { appState: connectedAppState, commands: { toggleLockout } },
    );

    await user.click(screen.getByRole("button", { name: "SAFE" }));

    expect(toggleLockout).toHaveBeenCalledTimes(1);
  });

  it("marks the SAFE button pressed when lockout is active", () => {
    renderTopBar({ ...connectedAppState, lockout: true });

    expect(screen.getByRole("button", { name: "SAFE" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });
});

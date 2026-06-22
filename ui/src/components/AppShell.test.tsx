import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { connectedAppState } from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import { AppShell } from "./AppShell";

describe("AppShell", () => {
  it("passes the open-connection handler to the top bar", async () => {
    const user = userEvent.setup();
    const onOpenConnection = vi.fn();

    renderWithAppProviders(
      <AppShell
        activeTab="scenes"
        onOpenConnection={onOpenConnection}
        onResume={vi.fn()}
        onSelectTab={vi.fn()}
        showConnection={false}
      />,
      { appState: connectedAppState },
    );

    await user.click(screen.getByRole("button", { name: /FOH LV1/i }));

    expect(onOpenConnection).toHaveBeenCalledTimes(1);
  });

  it("does not render the Sessions tab in the shell navigation", () => {
    renderWithAppProviders(
      <AppShell
        activeTab="scenes"
        onOpenConnection={vi.fn()}
        onResume={vi.fn()}
        onSelectTab={vi.fn()}
        showConnection={false}
      />,
      { appState: connectedAppState },
    );

    expect(
      screen.queryByRole("button", { name: "Sessions" }),
    ).not.toBeInTheDocument();
  });
});

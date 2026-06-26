import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  connectedAppState,
  discoveredSystemsAppState,
} from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import type { AppViewState, DiscoveredLv1System } from "../types";
import { ConnectionModal } from "./ConnectionModal";

function renderModal(
  options: {
    appState?: AppViewState;
    commandError?: string | null;
    onResume?: () => void;
    selectSystem?: (identity: DiscoveredLv1System["identity"]) => void;
  } = {},
) {
  return renderWithAppProviders(
    <ConnectionModal onResume={options.onResume ?? vi.fn()} />,
    {
      appState: options.appState ?? discoveredSystemsAppState,
      commandError: options.commandError,
      commands: options.selectSystem
        ? { selectSystem: options.selectSystem }
        : undefined,
    },
  );
}

describe("ConnectionModal", () => {
  it("renders discovered system details", () => {
    renderModal();

    expect(screen.getByText("FOH LV1")).toBeInTheDocument();
    expect(screen.getByText("192.168.1.42:22000")).toBeInTheDocument();
    expect(screen.getByText("Available")).toBeInTheDocument();
    expect(screen.getByText("LV1 Console")).toBeInTheDocument();
    expect(screen.getByText("192.168.1.43:22000")).toBeInTheDocument();
    expect(screen.getByText("Unavailable")).toBeInTheDocument();
  });

  it("shows command errors", () => {
    renderModal({ commandError: "LV1 did not connect" });

    expect(screen.getByText("LV1 did not connect")).toBeInTheDocument();
  });

  it("calls onResume from the close button", async () => {
    const user = userEvent.setup();
    const onResume = vi.fn();
    renderModal({ onResume });

    await user.click(screen.getByLabelText("Close connection modal"));

    expect(onResume).toHaveBeenCalledTimes(1);
  });

  it("selects available systems", async () => {
    const user = userEvent.setup();
    const selectSystem = vi.fn();
    renderModal({ selectSystem });

    await user.click(screen.getByRole("button", { name: /FOH LV1/i }));

    expect(selectSystem).toHaveBeenCalledWith({
      uuid: "lv1-demo",
      host: "FOH LV1",
      address: "192.168.1.42",
      port: 22000,
    });
  });

  it("does not select unavailable systems", async () => {
    const user = userEvent.setup();
    const selectSystem = vi.fn();
    renderModal({ selectSystem });

    await user.click(screen.getByRole("button", { name: /LV1 Console/i }));

    expect(selectSystem).not.toHaveBeenCalled();
  });

  it("highlights the currently connected system", () => {
    const appState: AppViewState = {
      ...connectedAppState,
      discoveredLv1Systems: [
        {
          identity: connectedAppState.connectedLv1Identity!,
          status: "connected",
        },
      ],
    };
    renderModal({ appState });

    expect(screen.getByRole("button", { name: /FOH LV1/i })).toHaveClass(
      "border-status-current",
    );
    expect(screen.getByText("Connected")).toBeInTheDocument();
  });
});

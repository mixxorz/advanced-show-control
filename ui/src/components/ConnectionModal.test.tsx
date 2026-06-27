import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  connectedAppState,
  discoveredSystemsAppState,
} from "../storybook/mockAppState";
import type { AppCommands } from "../appContext";
import { renderWithAppProviders } from "../test/render";
import type { AppViewState, DiscoveredLv1System } from "../types";
import { ConnectionModal } from "./ConnectionModal";

afterEach(() => {
  vi.useRealTimers();
});

function renderModal(
  options: {
    appState?: AppViewState;
    commandError?: string | null;
    onResume?: () => void;
    selectSystem?: (identity: DiscoveredLv1System["identity"]) => void;
    commands?: Partial<AppCommands>;
  } = {},
) {
  return renderWithAppProviders(
    <ConnectionModal onResume={options.onResume ?? vi.fn()} />,
    {
      appState: options.appState ?? discoveredSystemsAppState,
      commandError: options.commandError,
      commands: {
        probeLv1TcpConnectLatency: () => new Promise(() => {}),
        ...(options.selectSystem ? { selectSystem: options.selectSystem } : {}),
        ...(options.commands ?? {}),
      },
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

  it("updates TCP latency once per second without a separate test action", async () => {
    vi.useFakeTimers();
    let probeRound = 0;
    const probeLv1TcpConnectLatency = vi.fn(
      async (identity: DiscoveredLv1System["identity"]) => {
        const isFirstSystem = identity.address === "192.168.1.42";
        const tcpConnectMs = probeRound === 0 ? (isFirstSystem ? 5 : 8) : 13;
        if (!isFirstSystem) probeRound += 1;
        return { tcpConnectMs };
      },
    );
    renderModal({
      commands: { probeLv1TcpConnectLatency },
    });

    expect(
      screen.queryByRole("button", { name: "Test TCP latency" }),
    ).toBeNull();

    await act(async () => {
      await Promise.resolve();
    });

    expect(screen.getByText("5 ms")).toBeInTheDocument();
    expect(screen.getByText("8 ms")).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });

    await act(async () => {
      await Promise.resolve();
    });

    expect(probeLv1TcpConnectLatency).toHaveBeenCalledTimes(4);
    expect(screen.getAllByText("13 ms")).toHaveLength(2);
  });

  it("keeps selecting available systems while latency updates passively", async () => {
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

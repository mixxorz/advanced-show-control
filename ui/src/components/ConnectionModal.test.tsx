import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  connectedAppState,
  discoveredSystemsAppState,
} from "../storybook/mockAppState";
import type { AppCommands } from "../appContext";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { createDeferred } from "../test/deferred";
import { renderWithAppProviders } from "../test/render";
import type { AppViewState, DiscoveredLv1System } from "../types";
import { ConnectionModal } from "./ConnectionModal";

function renderModal(
  options: {
    appState?: AppViewState;
    commandError?: string | null;
    onResume?: () => void;
    selectSystem?: (identity: DiscoveredLv1System["identity"]) => Promise<void>;
    commands?: Partial<AppCommands>;
  } = {},
) {
  return renderWithAppProviders(
    <ConnectionModal onResume={options.onResume ?? vi.fn()} />,
    {
      appState: options.appState ?? discoveredSystemsAppState,
      commandError: options.commandError,
      commands: {
        ...(options.selectSystem ? { selectSystem: options.selectSystem } : {}),
        ...(options.commands ?? {}),
      },
    },
  );
}

describe("ConnectionModal", () => {
  it("renders an accessible dialog with discovered system details", () => {
    renderModal();

    expect(
      screen.getByRole("dialog", { name: "Connect to LV1" }),
    ).toHaveAttribute("aria-modal", "true");
    expect(screen.getByText("FOH LV1")).toBeInTheDocument();
    expect(screen.getByText("192.168.1.42:22000")).toBeInTheDocument();
    expect(screen.getByText("Available")).toBeInTheDocument();
    expect(screen.getByText("LV1 Console")).toBeInTheDocument();
    expect(screen.getByText("192.168.1.43:22000")).toBeInTheDocument();
    expect(screen.getByText("Unavailable")).toBeInTheDocument();
  });

  it("moves initial focus into the dialog", () => {
    renderModal();

    expect(screen.getByLabelText("Close connection modal")).toHaveFocus();
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
    const selectSystem = vi.fn(async () => undefined);
    renderModal({ selectSystem });

    await user.click(screen.getByRole("button", { name: "Select FOH LV1" }));

    expect(selectSystem).toHaveBeenCalledWith({
      uuid: "lv1-demo",
      host: "FOH LV1",
      address: "192.168.1.42",
      port: 22000,
    });
  });

  it("does not select unavailable systems", async () => {
    const user = userEvent.setup();
    const selectSystem = vi.fn(async () => undefined);
    renderModal({ selectSystem });

    await user.click(
      screen.getByRole("button", { name: "Select LV1 Console" }),
    );

    expect(selectSystem).not.toHaveBeenCalled();
  });

  it("only probes TCP latency when the row-local Test action is used", async () => {
    const user = userEvent.setup();
    const probeLv1TcpConnectLatency = vi.fn(async () => ({ tcpConnectMs: 5 }));
    renderModal({ commands: { probeLv1TcpConnectLatency } });

    expect(probeLv1TcpConnectLatency).not.toHaveBeenCalled();
    expect(screen.getAllByText("Not tested")).toHaveLength(2);

    await user.click(
      screen.getByRole("button", { name: "Test latency for FOH LV1" }),
    );

    expect(probeLv1TcpConnectLatency).toHaveBeenCalledTimes(1);
    expect(probeLv1TcpConnectLatency).toHaveBeenCalledWith({
      uuid: "lv1-demo",
      host: "FOH LV1",
      address: "192.168.1.42",
      port: 22000,
    });
    expect(screen.getByText("5 ms")).toBeInTheDocument();
  });

  it("clears row-local latency when a console identity changes", async () => {
    const user = userEvent.setup();
    const probeLv1TcpConnectLatency = vi.fn(async () => ({ tcpConnectMs: 5 }));
    const firstState: AppViewState = {
      ...discoveredSystemsAppState,
      discoveredLv1Systems: [discoveredSystemsAppState.discoveredLv1Systems[0]],
    };
    const commands = { probeLv1TcpConnectLatency };
    const { rerender } = render(
      <MockAppProviders appState={firstState} commands={commands}>
        <ConnectionModal onResume={vi.fn()} />
      </MockAppProviders>,
    );

    await user.click(
      screen.getByRole("button", { name: "Test latency for FOH LV1" }),
    );
    expect(screen.getByText("5 ms")).toBeInTheDocument();

    const changedState: AppViewState = {
      ...firstState,
      discoveredLv1Systems: [
        {
          ...firstState.discoveredLv1Systems[0],
          identity: {
            ...firstState.discoveredLv1Systems[0].identity,
            address: "192.168.1.99",
          },
        },
      ],
    };
    rerender(
      <MockAppProviders appState={changedState} commands={commands}>
        <ConnectionModal onResume={vi.fn()} />
      </MockAppProviders>,
    );

    expect(screen.queryByText("5 ms")).not.toBeInTheDocument();
    expect(screen.getByText("Not tested")).toBeInTheDocument();
  });

  it("guards a row against concurrent latency probes", async () => {
    const user = userEvent.setup();
    const probe = createDeferred<{ tcpConnectMs: number }>();
    const probeLv1TcpConnectLatency = vi.fn(() => probe.promise);
    renderModal({ commands: { probeLv1TcpConnectLatency } });
    const testButton = screen.getByRole("button", {
      name: "Test latency for FOH LV1",
    });

    await user.click(testButton);
    expect(testButton).toBeDisabled();
    expect(screen.getByText("Testing…")).toBeInTheDocument();
    await user.click(testButton);
    expect(probeLv1TcpConnectLatency).toHaveBeenCalledTimes(1);

    await act(async () => {
      probe.resolve({ tcpConnectMs: 7 });
      await probe.promise;
    });

    expect(testButton).toBeEnabled();
    expect(screen.getByText("7 ms")).toBeInTheDocument();
  });

  it("shows a row-local accessible latency error", async () => {
    const user = userEvent.setup();
    renderModal({
      commands: {
        probeLv1TcpConnectLatency: vi.fn(async () => {
          throw new Error("probe timed out");
        }),
      },
    });

    await user.click(
      screen.getByRole("button", { name: "Test latency for FOH LV1" }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Latency test failed: Error: probe timed out",
    );
  });

  it("guards a row against duplicate connection submissions", async () => {
    const user = userEvent.setup();
    const connection = createDeferred<void>();
    const selectSystem = vi.fn(() => connection.promise);
    renderModal({ selectSystem });
    const selectButton = screen.getByRole("button", {
      name: "Select FOH LV1",
    });

    await user.click(selectButton);
    expect(selectButton).toBeDisabled();
    await user.click(selectButton);
    expect(selectSystem).toHaveBeenCalledTimes(1);

    await act(async () => {
      connection.resolve();
      await connection.promise;
    });
    expect(selectButton).toBeEnabled();
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

    expect(screen.getByText("Connected")).toBeInTheDocument();
  });
});

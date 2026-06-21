import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderWithAppProviders } from "../test/render";
import { SmokeDebugApp } from "./SmokeDebugApp";

const commands = vi.hoisted(() => ({
  runConnectionTest: vi.fn(),
  runDecreasingXFadeTest: vi.fn(),
  runFadeCompletesTest: vi.fn(),
  runFadeStartsTest: vi.fn(),
  runLockoutBlocksRecallTest: vi.fn(),
  runSceneRecallTest: vi.fn(),
  refreshLv1Discovery: vi.fn(),
}));

vi.mock("./commands", () => commands);

describe("projectorChecks", () => {
  it("requests LV1 discovery when the debug app starts", async () => {
    commands.refreshLv1Discovery.mockResolvedValueOnce(undefined);

    renderWithAppProviders(
      <SmokeDebugApp
        services={{
          frontendReady: vi.fn(async () => undefined),
          listenForAppStatus: vi.fn(async () => () => {}),
        }}
      />,
    );

    expect(
      await screen.findByText("Searching for LV1 systems..."),
    ).toBeInTheDocument();
    expect(commands.refreshLv1Discovery).toHaveBeenCalled();
    expect(commands.runConnectionTest).not.toHaveBeenCalled();
  });

  it("renders backend and projector smoke steps for the latest result", async () => {
    const user = userEvent.setup();
    commands.runConnectionTest.mockResolvedValueOnce({
      ok: true,
      message: "connection ok",
      steps: [{ ok: true, step: "connect", message: "connected" }],
    });

    renderWithAppProviders(
      <SmokeDebugApp
        services={{
          frontendReady: vi.fn(async () => undefined),
          listenForAppStatus: vi.fn(async () => () => {}),
        }}
      />,
    );

    await user.type(screen.getByLabelText("LV1 address"), "127.0.0.1");
    await user.click(
      screen.getByLabelText("I understand this can move hardware faders"),
    );
    await user.click(
      screen.getByRole("button", { name: "Run Connection Test" }),
    );

    expect(screen.getByText("Backend Steps")).toBeInTheDocument();
    expect(screen.getAllByText("Backend Steps")).toHaveLength(1);
    expect(screen.getByText("PASS: connect - connected")).toBeInTheDocument();
    expect(screen.getByText("Projector Steps")).toBeInTheDocument();
    expect(screen.getAllByText("Projector Steps")).toHaveLength(1);
    expect(
      screen.getByText(
        /PASS: app-status-changed - connection projected from state version 0/,
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        /PASS: projected-logs - 0 projected log entries available/,
      ),
    ).toBeInTheDocument();
  });
});

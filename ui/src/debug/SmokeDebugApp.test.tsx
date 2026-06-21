import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AppStatusListener } from "../AppRuntime";
import { renderWithAppProviders } from "../test/render";
import { disconnectedAppViewState } from "../types";
import { SmokeDebugApp } from "./SmokeDebugApp";

const commands = vi.hoisted(() => ({
  runConnectionTest: vi.fn(),
  runDecreasingXFadeTest: vi.fn(),
  runFadeCompletesTest: vi.fn(),
  runFadeStartsTest: vi.fn(),
  runLockoutBlocksRecallTest: vi.fn(),
  runSceneRecallTest: vi.fn(),
  refreshLv1Discovery: vi.fn(),
  newShowFile: vi.fn(),
  storeSceneConfig: vi.fn(),
  setChannelScoped: vi.fn(),
  setSceneDurationMs: vi.fn(),
  setSmokeChannelGain: vi.fn(),
  reportSmokeSetup: vi.fn(),
  finishSmokeSuite: vi.fn(),
  exitSmokeApp: vi.fn(),
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

  it("exits the debug app after the automated smoke suite finishes", async () => {
    let appStatusListener: AppStatusListener | undefined;
    const passResult = (testId: string) => ({
      ok: true,
      testId,
      startedAt: "1",
      finishedAt: "2",
      steps: [],
      observedEvents: [],
      observedTraces: [],
    });
    commands.runConnectionTest.mockResolvedValueOnce(passResult("connection"));
    commands.newShowFile.mockResolvedValueOnce(undefined);
    commands.storeSceneConfig.mockResolvedValue(undefined);
    commands.setSmokeChannelGain.mockResolvedValue(undefined);
    commands.setChannelScoped.mockResolvedValue(undefined);
    commands.setSceneDurationMs.mockResolvedValue(undefined);
    commands.reportSmokeSetup.mockResolvedValue(undefined);
    commands.runSceneRecallTest.mockResolvedValueOnce(
      passResult("scene-recall"),
    );
    commands.runFadeStartsTest.mockResolvedValueOnce(passResult("fade-starts"));
    commands.runFadeCompletesTest.mockResolvedValueOnce(
      passResult("fade-completes"),
    );
    commands.runDecreasingXFadeTest.mockResolvedValueOnce(
      passResult("decreasing-xfade"),
    );
    commands.runLockoutBlocksRecallTest.mockResolvedValueOnce(
      passResult("lockout-blocks-recall"),
    );
    commands.finishSmokeSuite.mockResolvedValueOnce(undefined);
    commands.exitSmokeApp.mockResolvedValueOnce(undefined);

    renderWithAppProviders(
      <SmokeDebugApp
        services={{
          frontendReady: vi.fn(async () => undefined),
          listenForAppStatus: vi.fn(async (listener: AppStatusListener) => {
            appStatusListener = listener;
            return () => {};
          }),
        }}
      />,
    );

    await act(async () => {
      appStatusListener?.({
        ...disconnectedAppViewState,
        discoveredLv1Systems: [
          {
            identity: {
              uuid: "lv1-smoke",
              host: "Earth-1475",
              address: "127.0.0.1",
              port: 51927,
            },
            latencyMs: 1,
            status: "available",
          },
        ],
      });
    });

    await waitFor(() => expect(commands.finishSmokeSuite).toHaveBeenCalled());
    expect(commands.exitSmokeApp).toHaveBeenCalledTimes(1);
  });
});

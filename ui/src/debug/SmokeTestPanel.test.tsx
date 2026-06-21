import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { connectedAppState } from "../storybook/mockAppState";
import { disconnectedAppViewState } from "../types";
import { renderWithAppProviders } from "../test/render";
import { SmokeTestPanel } from "./SmokeTestPanel";

const discoveredAppState = {
  ...disconnectedAppViewState,
  discoveredLv1Systems: [
    {
      identity: {
        uuid: "lv1-uuid",
        host: "lv1-host",
        address: "192.168.10.50",
        port: 12345,
      },
      latencyMs: 2,
      status: "available" as const,
    },
  ],
  stateVersion: disconnectedAppViewState.stateVersion + 1,
};

describe("SmokeTestPanel", () => {
  it("invokes every smoke control when its inputs are valid", async () => {
    const user = userEvent.setup();
    const handlers = {
      onRunConnectionTest: vi.fn(),
      onRunSceneRecallTest: vi.fn(),
      onRunFadeStartsTest: vi.fn(),
      onRunFadeCompletesTest: vi.fn(),
      onRunDecreasingXFadeTest: vi.fn(),
      onRunLockoutBlocksRecallTest: vi.fn(),
    };

    renderWithAppProviders(
      <SmokeTestPanel
        appState={connectedAppState}
        onRefreshLv1Discovery={vi.fn(async () => undefined)}
        {...handlers}
      />,
    );

    await user.clear(screen.getByLabelText("LV1 address"));
    await user.type(screen.getByLabelText("LV1 address"), "127.0.0.1");
    await user.clear(screen.getByLabelText("LV1 port"));
    await user.type(screen.getByLabelText("LV1 port"), "12345");
    await user.type(screen.getByLabelText("Scene A"), "scene-a");
    await user.type(screen.getByLabelText("Scene B"), "scene-b");
    await user.type(screen.getByLabelText("Group"), "1");
    await user.type(screen.getByLabelText("Channel"), "2");
    await user.click(
      screen.getByLabelText("I understand this can move hardware faders"),
    );

    await user.click(
      screen.getByRole("button", { name: "Run Connection Test" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Run Scene Recall Test" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Run Fade Starts Test" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Run Fade Completes Test" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Run Decreasing XFade Test" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Run Lockout Blocks Recall Test" }),
    );

    expect(handlers.onRunConnectionTest).toHaveBeenCalledTimes(1);
    expect(handlers.onRunConnectionTest).toHaveBeenCalledWith({
      uuid: null,
      host: null,
      address: "127.0.0.1",
      port: 12345,
    });
    expect(handlers.onRunSceneRecallTest).toHaveBeenCalledTimes(1);
    expect(handlers.onRunFadeStartsTest).toHaveBeenCalledTimes(1);
    expect(handlers.onRunFadeCompletesTest).toHaveBeenCalledTimes(1);
    expect(handlers.onRunDecreasingXFadeTest).toHaveBeenCalledTimes(1);
    expect(handlers.onRunLockoutBlocksRecallTest).toHaveBeenCalledTimes(1);

    expect(handlers.onRunSceneRecallTest).toHaveBeenCalledWith(
      expect.objectContaining({
        sceneAId: "scene-a",
        sceneBId: "scene-b",
        channel: { group: 1, channel: 2 },
      }),
    );
  });

  it("auto-fills LV1 identity fields from the first discovered system", async () => {
    renderWithAppProviders(
      <SmokeTestPanel
        appState={discoveredAppState}
        onRefreshLv1Discovery={vi.fn(async () => undefined)}
        onRunConnectionTest={vi.fn()}
        onRunSceneRecallTest={vi.fn()}
        onRunFadeStartsTest={vi.fn()}
        onRunFadeCompletesTest={vi.fn()}
        onRunDecreasingXFadeTest={vi.fn()}
        onRunLockoutBlocksRecallTest={vi.fn()}
      />,
    );

    expect(screen.getByLabelText("LV1 address")).toHaveValue("192.168.10.50");
    expect(screen.getByLabelText("LV1 port")).toHaveValue("12345");
    expect(
      screen.getByText(
        /Auto-filled from discovered LV1: lv1-host 192\.168\.10\.50:12345/,
      ),
    ).toBeInTheDocument();
  });

  it("does not overwrite manually edited LV1 identity fields", async () => {
    const user = userEvent.setup();
    const onRefreshLv1Discovery = vi.fn(async () => undefined);
    const { rerender } = renderWithAppProviders(
      <SmokeTestPanel
        appState={disconnectedAppViewState}
        onRefreshLv1Discovery={onRefreshLv1Discovery}
        onRunConnectionTest={vi.fn()}
        onRunSceneRecallTest={vi.fn()}
        onRunFadeStartsTest={vi.fn()}
        onRunFadeCompletesTest={vi.fn()}
        onRunDecreasingXFadeTest={vi.fn()}
        onRunLockoutBlocksRecallTest={vi.fn()}
      />,
    );

    await user.type(screen.getByLabelText("LV1 address"), "10.0.0.9");

    rerender(
      <SmokeTestPanel
        appState={discoveredAppState}
        onRefreshLv1Discovery={onRefreshLv1Discovery}
        onRunConnectionTest={vi.fn()}
        onRunSceneRecallTest={vi.fn()}
        onRunFadeStartsTest={vi.fn()}
        onRunFadeCompletesTest={vi.fn()}
        onRunDecreasingXFadeTest={vi.fn()}
        onRunLockoutBlocksRecallTest={vi.fn()}
      />,
    );

    expect(screen.getByLabelText("LV1 address")).toHaveValue("10.0.0.9");
    expect(
      screen.getByText("Manual LV1 identity entry active."),
    ).toBeInTheDocument();
  });

  it("does not connect or run smoke tests when auto-fill succeeds", () => {
    const handlers = {
      onRunConnectionTest: vi.fn(),
      onRunSceneRecallTest: vi.fn(),
      onRunFadeStartsTest: vi.fn(),
      onRunFadeCompletesTest: vi.fn(),
      onRunDecreasingXFadeTest: vi.fn(),
      onRunLockoutBlocksRecallTest: vi.fn(),
    };

    renderWithAppProviders(
      <SmokeTestPanel
        appState={discoveredAppState}
        onRefreshLv1Discovery={vi.fn(async () => undefined)}
        {...handlers}
      />,
    );

    expect(handlers.onRunConnectionTest).not.toHaveBeenCalled();
    expect(handlers.onRunSceneRecallTest).not.toHaveBeenCalled();
    expect(handlers.onRunFadeStartsTest).not.toHaveBeenCalled();
    expect(handlers.onRunFadeCompletesTest).not.toHaveBeenCalled();
    expect(handlers.onRunDecreasingXFadeTest).not.toHaveBeenCalled();
    expect(handlers.onRunLockoutBlocksRecallTest).not.toHaveBeenCalled();
  });
});

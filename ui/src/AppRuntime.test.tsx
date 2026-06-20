import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AppRuntime, type AppRuntimeServices } from "./AppRuntime";
import { connectedAppState } from "./storybook/mockAppState";
import { createDeferred } from "./test/deferred";
import { disconnectedAppViewState, type AppViewState } from "./types";

function makeServices(
  overrides: Partial<AppRuntimeServices> = {},
): AppRuntimeServices {
  return {
    frontendReady: vi.fn(async () => undefined),
    abortAll: vi.fn(async () => undefined),
    attemptReconnectLv1: vi.fn(async () => undefined),
    connectLv1System: vi.fn(async () => undefined),
    disconnectLv1: vi.fn(async () => undefined),
    listenForAppStatus: vi.fn(async (listener) => {
      listener(connectedAppState);
      return () => {};
    }),
    newShowFile: vi.fn(async () => undefined),
    openShowFile: vi.fn(async () => undefined),
    cueScene: vi.fn(async () => undefined),
    recallScene: vi.fn(async () => undefined),
    reconnectTimedOut: vi.fn(async () => undefined),
    refreshLv1Discovery: vi.fn(async () => undefined),
    saveShowFile: vi.fn(async () => undefined),
    saveShowFileAs: vi.fn(async () => undefined),
    selectSceneConfig: vi.fn(async () => undefined),
    setAllChannelsScoped: vi.fn(async () => undefined),
    setChannelScoped: vi.fn(async () => undefined),
    setLockout: vi.fn(async () => undefined),
    setSceneDurationMs: vi.fn(async () => undefined),
    setSceneScopeFadersEnabled: vi.fn(async () => undefined),
    setSceneScopePanEnabled: vi.fn(async () => undefined),
    storeSceneConfig: vi.fn(async () => undefined),
    startupAutoConnectLv1: vi.fn(async () => undefined),
    ...overrides,
  };
}

describe("AppRuntime connection lifecycle", () => {
  it("opens the connection modal on startup", () => {
    render(
      <AppRuntime
        services={makeServices({
          listenForAppStatus: vi.fn(async () => () => {}),
          startupAutoConnectLv1: vi.fn(async () => undefined),
        })}
      />,
    );

    expect(
      screen.getByRole("heading", { name: "Connect to LV1" }),
    ).toBeInTheDocument();
  });

  it("closes the modal after successful startup auto-connect", async () => {
    let listener: ((snapshot: AppViewState) => void) | null = null;
    render(
      <AppRuntime
        services={makeServices({
          listenForAppStatus: vi.fn(async (next) => {
            listener = next;
            return () => {};
          }),
        })}
      />,
    );

    await act(async () => {
      listener?.(connectedAppState);
    });

    await waitFor(() => {
      expect(
        screen.queryByRole("heading", { name: "Connect to LV1" }),
      ).not.toBeInTheDocument();
    });
  });

  it("keeps the modal open and displays startup auto-connect errors", async () => {
    render(
      <AppRuntime
        services={makeServices({
          startupAutoConnectLv1: vi.fn(async () => {
            throw new Error("startup failed");
          }),
        })}
      />,
    );

    expect(
      await screen.findByText("Error: startup failed"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Connect to LV1" }),
    ).toBeInTheDocument();
  });

  it("allows the engineer to close the modal and stay offline", async () => {
    const user = userEvent.setup();
    render(
      <AppRuntime
        services={makeServices({
          listenForAppStatus: vi.fn(async () => () => {}),
          startupAutoConnectLv1: vi.fn(async () => undefined),
        })}
      />,
    );

    await user.click(screen.getByLabelText("Close connection modal"));

    expect(
      screen.queryByRole("heading", { name: "Connect to LV1" }),
    ).not.toBeInTheDocument();

    expect(
      within(screen.getByRole("contentinfo")).getByText("Offline"),
    ).toBeInTheDocument();
  });

  it("ignores an equal-version stale snapshot after initialization", async () => {
    let appStatusListener: ((snapshot: AppViewState) => void) | null = null;
    const services = makeServices({
      startupAutoConnectLv1: vi.fn(async () => undefined),
      listenForAppStatus: vi.fn(async (listener) => {
        appStatusListener = listener;
        return () => {};
      }),
    });

    render(<AppRuntime services={services} />);

    await act(async () => {
      appStatusListener?.(connectedAppState);
    });

    await waitFor(() => {
      expect(
        screen.queryByRole("heading", { name: "Connect to LV1" }),
      ).not.toBeInTheDocument();
    });

    await act(async () => {
      appStatusListener?.({
        ...disconnectedAppViewState,
        stateVersion: connectedAppState.stateVersion,
      });
    });

    expect(screen.getByText("Connected")).toBeInTheDocument();
    expect(screen.queryByText("Offline")).not.toBeInTheDocument();
  });

  it("keeps the modal closed when a stale startup snapshot resolves after a newer connected status", async () => {
    let appStatusListener: ((snapshot: AppViewState) => void) | null = null;
    const services = makeServices({
      listenForAppStatus: vi.fn(async (listener) => {
        appStatusListener = listener;
        return () => {};
      }),
    });

    render(<AppRuntime services={services} />);

    await waitFor(() => {
      expect(appStatusListener).not.toBeNull();
    });

    await act(async () => {
      appStatusListener?.(connectedAppState);
    });

    await waitFor(() => {
      expect(
        screen.queryByRole("heading", { name: "Connect to LV1" }),
      ).not.toBeInTheDocument();
    });
    expect(
      screen.queryByRole("heading", { name: "Connect to LV1" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Connected")).toBeInTheDocument();
    expect(screen.queryByText("Offline")).not.toBeInTheDocument();
  });

  it("keeps the modal open when the engineer opens it while connected", async () => {
    const user = userEvent.setup();
    render(<AppRuntime services={makeServices()} />);

    await waitFor(() => {
      expect(
        screen.queryByRole("heading", { name: "Connect to LV1" }),
      ).not.toBeInTheDocument();
    });

    await user.click(screen.getByRole("button", { name: /FOH LV1/i }));

    expect(
      screen.getByRole("heading", { name: "Connect to LV1" }),
    ).toBeInTheDocument();
  });

  it("keeps a manually opened modal open when reconnect succeeds", async () => {
    const user = userEvent.setup();
    const reconnect = createDeferred<AppViewState>();
    let appStatusListener: ((snapshot: AppViewState) => void) | null = null;
    const services = makeServices({
      startupAutoConnectLv1: vi.fn(async () => undefined),
      listenForAppStatus: vi.fn(async (listener) => {
        appStatusListener = listener;
        return () => {};
      }),
      attemptReconnectLv1: vi.fn(() => reconnect.promise),
    });
    render(<AppRuntime services={services} />);

    await act(async () => {
      appStatusListener?.(connectedAppState);
    });

    await waitFor(() => {
      expect(
        screen.queryByRole("heading", { name: "Connect to LV1" }),
      ).not.toBeInTheDocument();
    });

    await act(async () => {
      appStatusListener?.({
        ...connectedAppState,
        reconnect: { active: true, attempt: 1 },
        stateVersion: connectedAppState.stateVersion + 1,
      });
    });

    await user.click(screen.getByRole("button", { name: /FOH LV1/i }));

    await act(async () => {
      reconnect.resolve({
        ...connectedAppState,
        reconnect: { active: false, attempt: 1 },
        stateVersion: connectedAppState.stateVersion + 2,
      });
      await reconnect.promise;
    });

    expect(
      screen.getByRole("heading", { name: "Connect to LV1" }),
    ).toBeInTheDocument();
  });

  it("wires cue recall and go buttons to runtime services", async () => {
    const user = userEvent.setup();
    const services = makeServices({
      startupAutoConnectLv1: vi.fn(async () => undefined),
    });
    const scene = connectedAppState.sceneConfigs[0];
    render(<AppRuntime services={services} />);

    await waitFor(() => {
      expect(
        screen.queryByRole("heading", { name: "Connect to LV1" }),
      ).not.toBeInTheDocument();
    });

    await user.click(screen.getByRole("button", { name: "Cue" }));
    await user.click(screen.getByRole("button", { name: "Recall" }));
    await user.click(screen.getByRole("button", { name: "GO" }));

    expect(services.cueScene).toHaveBeenCalledWith(scene.sceneId);
    expect(services.recallScene).toHaveBeenCalledWith(scene.sceneId);
    expect(services.recallScene).toHaveBeenCalledTimes(1);
  });

  it("registers app-status listener before signaling frontend readiness", async () => {
    const calls: string[] = [];
    render(
      <AppRuntime
        services={makeServices({
          listenForAppStatus: vi.fn(async () => {
            calls.push("listen");
            return () => {};
          }),
          startupAutoConnectLv1: vi.fn(async () => undefined),
          frontendReady: vi.fn(async () => {
            calls.push("ready");
          }),
        })}
      />,
    );

    await waitFor(() => expect(calls).toEqual(["listen", "ready"]));
  });

  it("does not apply command return values as app state", async () => {
    const user = userEvent.setup();
    let listener: ((snapshot: AppViewState) => void) | null = null;
    const sentinel: AppViewState = {
      ...disconnectedAppViewState,
      showFileName: "COMMAND_RESULT_SENTINEL_SHOULD_NOT_RENDER.lv1show",
      stateVersion: disconnectedAppViewState.stateVersion + 1,
    };
    const services = makeServices({
      listenForAppStatus: vi.fn(async (next) => {
        listener = next;
        return () => {};
      }),
      frontendReady: vi.fn(async () => undefined),
      newShowFile: vi.fn(async () => sentinel),
    });
    render(<AppRuntime services={services} />);

    await user.click(screen.getByLabelText("Close connection modal"));
    await user.click(screen.getByRole("button", { name: "Sessions" }));
    await user.click(screen.getByRole("button", { name: "New" }));

    expect(
      screen.queryByText("COMMAND_RESULT_SENTINEL_SHOULD_NOT_RENDER.lv1show"),
    ).not.toBeInTheDocument();

    await act(async () => {
      listener?.(sentinel);
    });

    expect(
      await screen.findByText(
        "COMMAND_RESULT_SENTINEL_SHOULD_NOT_RENDER.lv1show",
      ),
    ).toBeInTheDocument();
  });
});

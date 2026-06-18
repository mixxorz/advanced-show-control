import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AppRuntime, type AppRuntimeServices } from "./AppRuntime";
import {
  connectedAppState,
  discoveredSystemsAppState,
} from "./storybook/mockAppState";
import { createDeferred } from "./test/deferred";
import { disconnectedAppViewState, type AppViewState } from "./types";
import { render } from "@testing-library/react";

function makeServices(
  overrides: Partial<AppRuntimeServices> = {},
): AppRuntimeServices {
  return {
    abortAll: vi.fn(async () => undefined),
    attemptReconnectLv1: vi.fn(async () => disconnectedAppViewState),
    connectLv1System: vi.fn(async () => connectedAppState),
    disconnectLv1: vi.fn(async () => disconnectedAppViewState),
    listenForAppStatus: vi.fn(async () => () => {}),
    newShowFile: vi.fn(async () => disconnectedAppViewState),
    openShowFile: vi.fn(async () => disconnectedAppViewState),
    cueScene: vi.fn(async () => disconnectedAppViewState),
    recallScene: vi.fn(async () => disconnectedAppViewState),
    reconnectTimedOut: vi.fn(async () => disconnectedAppViewState),
    refreshAppState: vi.fn(async () => disconnectedAppViewState),
    refreshLv1Discovery: vi.fn(async () => discoveredSystemsAppState),
    saveShowFile: vi.fn(async () => disconnectedAppViewState),
    saveShowFileAs: vi.fn(async () => disconnectedAppViewState),
    selectSceneConfig: vi.fn(async () => disconnectedAppViewState),
    setAllChannelsScoped: vi.fn(async () => disconnectedAppViewState),
    setChannelScoped: vi.fn(async () => disconnectedAppViewState),
    setLockout: vi.fn(async () => disconnectedAppViewState),
    setSceneDurationMs: vi.fn(async () => disconnectedAppViewState),
    setSceneScopeFadersEnabled: vi.fn(async () => disconnectedAppViewState),
    setSceneScopePanEnabled: vi.fn(async () => disconnectedAppViewState),
    storeSceneConfig: vi.fn(async () => disconnectedAppViewState),
    startupAutoConnectLv1: vi.fn(async () => disconnectedAppViewState),
    ...overrides,
  };
}

describe("AppRuntime connection lifecycle", () => {
  it("opens the connection modal on startup", () => {
    render(<AppRuntime services={makeServices()} />);

    expect(
      screen.getByRole("heading", { name: "Connect to LV1" }),
    ).toBeInTheDocument();
  });

  it("closes the modal after successful startup auto-connect", async () => {
    render(
      <AppRuntime
        services={makeServices({
          startupAutoConnectLv1: vi.fn(async () => connectedAppState),
        })}
      />,
    );

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

  it("keeps the modal open while manual connect is pending and closes after selected system connects", async () => {
    const user = userEvent.setup();
    const pending = createDeferred<AppViewState>();
    const services = makeServices({
      startupAutoConnectLv1: vi.fn(async () => discoveredSystemsAppState),
      connectLv1System: vi.fn(() => pending.promise),
    });
    render(<AppRuntime services={services} />);
    await screen.findByText("FOH LV1");

    await user.click(screen.getByRole("button", { name: /FOH LV1/i }));

    expect(
      screen.getByRole("heading", { name: "Connect to LV1" }),
    ).toBeInTheDocument();

    await act(async () => {
      pending.resolve(connectedAppState);
      await pending.promise;
    });

    await waitFor(() => {
      expect(
        screen.queryByRole("heading", { name: "Connect to LV1" }),
      ).not.toBeInTheDocument();
    });
  });

  it("keeps the modal open and displays manual connection errors", async () => {
    const user = userEvent.setup();
    const services = makeServices({
      startupAutoConnectLv1: vi.fn(async () => discoveredSystemsAppState),
      connectLv1System: vi.fn(async () => {
        throw new Error("manual failed");
      }),
    });
    render(<AppRuntime services={services} />);
    await screen.findByText("FOH LV1");

    await user.click(screen.getByRole("button", { name: /FOH LV1/i }));

    expect(await screen.findByText("Error: manual failed")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Connect to LV1" }),
    ).toBeInTheDocument();
  });

  it("allows the engineer to close the modal and stay offline", async () => {
    const user = userEvent.setup();
    render(<AppRuntime services={makeServices()} />);

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
      startupAutoConnectLv1: vi.fn(async () => connectedAppState),
      listenForAppStatus: vi.fn(async (listener) => {
        appStatusListener = listener;
        return () => {};
      }),
    });

    render(<AppRuntime services={services} />);

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
    const startup = createDeferred<AppViewState>();
    let appStatusListener: ((snapshot: AppViewState) => void) | null = null;
    const services = makeServices({
      startupAutoConnectLv1: vi.fn(() => startup.promise),
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

    await act(async () => {
      startup.resolve(discoveredSystemsAppState);
      await startup.promise;
    });

    expect(
      screen.queryByRole("heading", { name: "Connect to LV1" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Connected")).toBeInTheDocument();
    expect(screen.queryByText("Offline")).not.toBeInTheDocument();
  });

  it("closes the modal when startup auto-connect resolves connected after discovery refresh", async () => {
    const startup = createDeferred<AppViewState>();
    let appStatusListener: ((snapshot: AppViewState) => void) | null = null;
    const laterDiscoverySnapshot: AppViewState = {
      ...discoveredSystemsAppState,
      stateVersion: connectedAppState.stateVersion + 1,
    };
    const services = makeServices({
      startupAutoConnectLv1: vi.fn(() => startup.promise),
      listenForAppStatus: vi.fn(async (listener) => {
        appStatusListener = listener;
        return () => {};
      }),
      refreshLv1Discovery: vi.fn(async () => laterDiscoverySnapshot),
    });

    render(<AppRuntime services={services} />);

    await screen.findByText("FOH LV1");

    await act(async () => {
      appStatusListener?.({
        ...connectedAppState,
        stateVersion: laterDiscoverySnapshot.stateVersion + 1,
      });
    });

    await act(async () => {
      startup.resolve(connectedAppState);
      await startup.promise;
    });

    await waitFor(() => {
      expect(
        screen.queryByRole("heading", { name: "Connect to LV1" }),
      ).not.toBeInTheDocument();
    });
    expect(screen.getByText("Connected")).toBeInTheDocument();
  });

  it("keeps the modal open when the engineer opens it while connected", async () => {
    const user = userEvent.setup();
    render(
      <AppRuntime
        services={makeServices({
          startupAutoConnectLv1: vi.fn(async () => connectedAppState),
        })}
      />,
    );

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
      startupAutoConnectLv1: vi.fn(async () => connectedAppState),
      listenForAppStatus: vi.fn(async (listener) => {
        appStatusListener = listener;
        return () => {};
      }),
      attemptReconnectLv1: vi.fn(() => reconnect.promise),
    });
    render(<AppRuntime services={services} />);

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
      startupAutoConnectLv1: vi.fn(async () => ({
        ...connectedAppState,
        cuedSceneId: connectedAppState.sceneConfigs[0]?.sceneId ?? null,
      })),
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
    expect(services.recallScene).toHaveBeenCalledTimes(2);
  });
});

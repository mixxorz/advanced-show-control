import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  AppCommandsProvider,
  AppStateProvider,
  type AppCommands,
} from "../appContext";
import { connectedAppState } from "../storybook/mockAppState";
import type { AppViewState } from "../types";
import { SceneEditor } from "./SceneEditor";

function makeCommands(commands: Partial<AppCommands> = {}): AppCommands {
  return {
    abortAll: vi.fn(),
    disconnect: vi.fn(),
    newShowFile: vi.fn(),
    openShowFile: vi.fn(),
    probeLv1TcpConnectLatency: vi.fn(),
    saveShowFile: vi.fn(),
    saveShowFileAs: vi.fn(),
    selectScene: vi.fn(),
    selectSystem: vi.fn(),
    setAllChannelsScoped: vi.fn(),
    setChannelScoped: vi.fn(),
    setSceneDurationMs: vi.fn(),
    setSceneScopeFadersEnabled: vi.fn(),
    setSceneScopePanEnabled: vi.fn(),
    storeSceneConfig: vi.fn(),
    linkSceneConfig: vi.fn(),
    deleteSceneConfig: vi.fn(),
    toggleLockout: vi.fn(),
    ...commands,
  };
}

function editorTree(
  appState: AppViewState,
  commands: Partial<AppCommands> = {},
) {
  return (
    <AppStateProvider appState={appState} commandError={null}>
      <AppCommandsProvider commands={makeCommands(commands)}>
        <SceneEditor />
      </AppCommandsProvider>
    </AppStateProvider>
  );
}

function renderEditor(
  appState: AppViewState = connectedAppState,
  commands: Partial<AppCommands> = {},
) {
  return render(editorTree(appState, commands));
}

describe("SceneEditor", () => {
  it("disables Store Cue and Recall for unlinked scenes", () => {
    renderEditor({
      ...connectedAppState,
      selectedSceneInternalId:
        connectedAppState.sceneConfigs[0].internalSceneId,
      sceneConfigs: [
        { ...connectedAppState.sceneConfigs[0], sceneIndex: null },
      ],
    });

    expect(screen.getByRole("button", { name: "Store" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cue" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Recall" })).toBeDisabled();
  });

  it("keeps duration and scope controls enabled for unlinked scenes", () => {
    renderEditor({
      ...connectedAppState,
      selectedSceneInternalId:
        connectedAppState.sceneConfigs[0].internalSceneId,
      sceneConfigs: [
        { ...connectedAppState.sceneConfigs[0], sceneIndex: null },
      ],
    });

    expect(screen.getByDisplayValue("2.5s")).toBeEnabled();
    expect(screen.getByRole("button", { name: "All" })).toBeEnabled();
  });

  it("shows link and delete controls for unlinked scenes", () => {
    renderEditor({
      ...connectedAppState,
      selectedSceneInternalId:
        connectedAppState.sceneConfigs[0].internalSceneId,
      sceneConfigs: [
        { ...connectedAppState.sceneConfigs[0], sceneIndex: null },
      ],
    });

    expect(
      screen.getByRole("button", { name: "Link to scene" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delete" })).toBeInTheDocument();
  });

  it("confirms overwrite in-app when linking to a scene with an existing config", async () => {
    const user = userEvent.setup();
    const linkSceneConfig = vi.fn();

    renderEditor(
      {
        ...connectedAppState,
        selectedSceneInternalId:
          connectedAppState.sceneConfigs[0].internalSceneId,
        sceneConfigs: [
          { ...connectedAppState.sceneConfigs[0], sceneIndex: null },
          { ...connectedAppState.sceneConfigs[1], sceneIndex: 0 },
        ],
      },
      { linkSceneConfig },
    );

    await user.selectOptions(screen.getByLabelText("LV1 Scene"), "0");
    await user.click(screen.getByRole("button", { name: "Link to scene" }));

    expect(screen.getByRole("dialog")).toHaveTextContent(
      "Overwrite Existing Fade Settings?",
    );

    await user.click(screen.getByRole("button", { name: "Overwrite" }));

    expect(linkSceneConfig).toHaveBeenCalledWith(
      connectedAppState.sceneConfigs[0].internalSceneId,
      0,
      true,
    );
  });

  it("links an unlinked scene to the first available LV1 scene by default", async () => {
    const user = userEvent.setup();
    const linkSceneConfig = vi.fn();

    renderEditor(
      {
        ...connectedAppState,
        selectedSceneInternalId:
          connectedAppState.sceneConfigs[0].internalSceneId,
        sceneConfigs: [
          { ...connectedAppState.sceneConfigs[0], sceneIndex: null },
          { ...connectedAppState.sceneConfigs[1], sceneIndex: 0 },
        ],
      },
      { linkSceneConfig },
    );

    await user.click(screen.getByRole("button", { name: "Link to scene" }));

    expect(linkSceneConfig).toHaveBeenCalledWith(
      connectedAppState.sceneConfigs[0].internalSceneId,
      1,
      false,
    );
  });

  it("links to an LV1 scene that appears after the unlinked controls mount", async () => {
    const user = userEvent.setup();
    const linkSceneConfig = vi.fn();
    const appState = {
      ...connectedAppState,
      selectedSceneInternalId:
        connectedAppState.sceneConfigs[0].internalSceneId,
      scenes: [],
      sceneConfigs: [
        { ...connectedAppState.sceneConfigs[0], sceneIndex: null },
      ],
    };

    const { rerender } = renderEditor(appState, { linkSceneConfig });

    rerender(
      editorTree(
        { ...appState, scenes: connectedAppState.scenes },
        { linkSceneConfig },
      ),
    );

    await user.click(screen.getByRole("button", { name: "Link to scene" }));

    expect(linkSceneConfig).toHaveBeenCalledWith(
      connectedAppState.sceneConfigs[0].internalSceneId,
      0,
      false,
    );
  });
});

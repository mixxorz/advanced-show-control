import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AppCommandsProvider, AppStateProvider } from "../appContext";
import { connectedAppState } from "../storybook/mockAppState";
import { SceneEditor } from "./SceneEditor";

function renderEditor(appState = connectedAppState, commands = {}) {
  return render(
    <AppStateProvider appState={appState} commandError={null}>
      <AppCommandsProvider
        commands={{
          abortAll: vi.fn(),
          disconnect: vi.fn(),
          newShowFile: vi.fn(),
          openShowFile: vi.fn(),
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
        }}
      >
        <SceneEditor />
      </AppCommandsProvider>
    </AppStateProvider>,
  );
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
      screen.getByRole("button", { name: "Link to LV1 Scene" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delete" })).toBeInTheDocument();
  });

  it("shows overwrite confirmation when linking to a scene with an existing config", async () => {
    const user = userEvent.setup();
    const confirmSpy = vi.fn(() => true);
    vi.stubGlobal("confirm", confirmSpy);
    const store = vi.fn();

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
      { linkSceneConfig: vi.fn(), storeSceneConfig: store },
    );

    await user.selectOptions(screen.getByLabelText("LV1 Scene"), "0");
    await user.click(screen.getByRole("button", { name: "Link to LV1 Scene" }));

    expect(confirmSpy).toHaveBeenCalled();
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

    await user.click(screen.getByRole("button", { name: "Link to LV1 Scene" }));

    expect(linkSceneConfig).toHaveBeenCalledWith(
      connectedAppState.sceneConfigs[0].internalSceneId,
      1,
      false,
    );
  });
});

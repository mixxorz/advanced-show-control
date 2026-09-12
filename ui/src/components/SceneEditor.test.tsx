import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  AppCommandsProvider,
  AppStateProvider,
  type AppCommands,
} from "../appContext";
import { mockAppCommands } from "../storybook/mockAppCommands";
import { connectedAppState } from "../storybook/mockAppState";
import type { AppViewState } from "../types";
import { SceneEditor } from "./SceneEditor";

function makeCommands(commands: Partial<AppCommands> = {}): AppCommands {
  return { ...mockAppCommands, ...commands };
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
  it("disables Store and Recall for unlinked scenes", () => {
    renderEditor({
      ...connectedAppState,
      selectedSceneInternalId:
        connectedAppState.sceneConfigs[0].internalSceneId,
      sceneConfigs: [
        { ...connectedAppState.sceneConfigs[0], sceneIndex: null },
      ],
    });

    expect(screen.getByRole("button", { name: "Store" })).toBeDisabled();
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

  it("marks the selected scene as cued with a named identity region only when the active cue list points to it", () => {
    const selectedScene = connectedAppState.sceneConfigs[0];

    const { rerender } = renderEditor({
      ...connectedAppState,
      selectedSceneInternalId: selectedScene.internalSceneId,
      activeCueListId: "cue-list-main",
      cuedCueEntryId: "cue-2",
    });

    expect(
      screen.getByRole("group", { name: "Selected scene" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("group", { name: "Selected scene, cued" }),
    ).not.toBeInTheDocument();

    rerender(
      editorTree({
        ...connectedAppState,
        selectedSceneInternalId: selectedScene.internalSceneId,
        activeCueListId: "cue-list-main",
        cuedCueEntryId: "cue-1",
      }),
    );

    expect(
      screen.getByRole("group", { name: "Selected scene, cued" }),
    ).toBeInTheDocument();
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

    expect(
      screen.getByRole("dialog", {
        name: "Overwrite Existing Fade Settings?",
      }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Overwrite" }));

    expect(linkSceneConfig).toHaveBeenCalledWith(
      connectedAppState.sceneConfigs[0].internalSceneId,
      0,
      true,
    );
  });

  it("does not confirm an overwrite after its LV1 target disappears", async () => {
    const user = userEvent.setup();
    const linkSceneConfig = vi.fn();
    const selectedScene = {
      ...connectedAppState.sceneConfigs[0],
      sceneIndex: null,
    };
    const initialState = {
      ...connectedAppState,
      selectedSceneInternalId: selectedScene.internalSceneId,
      sceneConfigs: [
        selectedScene,
        { ...connectedAppState.sceneConfigs[1], sceneIndex: 0 },
      ],
    };

    const { rerender } = renderEditor(initialState, { linkSceneConfig });
    await user.selectOptions(screen.getByLabelText("LV1 Scene"), "0");
    await user.click(screen.getByRole("button", { name: "Link to scene" }));

    rerender(
      editorTree(
        { ...initialState, scenes: initialState.scenes.slice(1) },
        { linkSceneConfig },
      ),
    );

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByText(/Unknown/)).not.toBeInTheDocument();
    expect(linkSceneConfig).not.toHaveBeenCalled();
  });

  it("does not resurrect pending overwrite intent after selecting source A, then B, then A", async () => {
    const user = userEvent.setup();
    const linkSceneConfig = vi.fn();
    const firstSource = {
      ...connectedAppState.sceneConfigs[0],
      internalSceneId: "source-1",
      sceneName: "First Source",
      sceneIndex: null,
    };
    const secondSource = {
      ...firstSource,
      internalSceneId: "source-2",
      sceneName: "Second Source",
    };
    const conflictingScene = {
      ...connectedAppState.sceneConfigs[1],
      sceneIndex: 0,
    };
    const initialState = {
      ...connectedAppState,
      selectedSceneInternalId: firstSource.internalSceneId,
      sceneConfigs: [firstSource, secondSource, conflictingScene],
    };

    const { rerender } = renderEditor(initialState, { linkSceneConfig });
    await user.selectOptions(screen.getByLabelText("LV1 Scene"), "0");
    await user.click(screen.getByRole("button", { name: "Link to scene" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    rerender(
      editorTree(
        {
          ...initialState,
          selectedSceneInternalId: secondSource.internalSceneId,
        },
        { linkSceneConfig },
      ),
    );

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });

    rerender(editorTree(initialState, { linkSceneConfig }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(linkSceneConfig).not.toHaveBeenCalled();
  });

  it("does not transfer pending overwrite intent to a replacement target at the same index", async () => {
    const user = userEvent.setup();
    const linkSceneConfig = vi.fn();
    const selectedScene = {
      ...connectedAppState.sceneConfigs[0],
      sceneIndex: null,
    };
    const conflictingScene = {
      ...connectedAppState.sceneConfigs[1],
      sceneIndex: 0,
    };
    const initialState = {
      ...connectedAppState,
      selectedSceneInternalId: selectedScene.internalSceneId,
      sceneConfigs: [selectedScene, conflictingScene],
    };

    const { rerender } = renderEditor(initialState, { linkSceneConfig });
    await user.selectOptions(screen.getByLabelText("LV1 Scene"), "0");
    await user.click(screen.getByRole("button", { name: "Link to scene" }));
    expect(screen.getByRole("dialog")).toHaveTextContent(
      initialState.scenes[0].name,
    );

    rerender(
      editorTree(
        {
          ...initialState,
          scenes: [
            { ...initialState.scenes[0], name: "Replacement Scene" },
            ...initialState.scenes.slice(1),
          ],
        },
        { linkSceneConfig },
      ),
    );

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByText("Replacement Scene")).not.toBeInTheDocument();
    expect(linkSceneConfig).not.toHaveBeenCalled();
  });

  it("uses the latest conflict state when confirming a pending overwrite", async () => {
    const user = userEvent.setup();
    const linkSceneConfig = vi.fn();
    const selectedScene = {
      ...connectedAppState.sceneConfigs[0],
      sceneIndex: null,
    };
    const conflictingScene = {
      ...connectedAppState.sceneConfigs[1],
      sceneIndex: 0,
    };
    const initialState = {
      ...connectedAppState,
      selectedSceneInternalId: selectedScene.internalSceneId,
      sceneConfigs: [selectedScene, conflictingScene],
    };

    const { rerender } = renderEditor(initialState, { linkSceneConfig });
    await user.selectOptions(screen.getByLabelText("LV1 Scene"), "0");
    await user.click(screen.getByRole("button", { name: "Link to scene" }));

    rerender(
      editorTree(
        { ...initialState, sceneConfigs: [selectedScene] },
        { linkSceneConfig },
      ),
    );
    await user.click(screen.getByRole("button", { name: "Overwrite" }));

    expect(linkSceneConfig).toHaveBeenCalledWith(
      selectedScene.internalSceneId,
      0,
      false,
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

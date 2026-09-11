import { screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { connectedAppState } from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import { disconnectedAppViewState, type AppViewState } from "../types";
import { BottomStatusBar } from "./BottomStatusBar";

function renderBottomStatusBar(
  appState: AppViewState,
  options: Parameters<typeof renderWithAppProviders>[1] = {},
) {
  renderWithAppProviders(<BottomStatusBar appState={appState} />, {
    appState,
    ...options,
  });
}

describe("BottomStatusBar", () => {
  it("shows dashes when no current or cued scene is available", () => {
    renderBottomStatusBar({
      ...connectedAppState,
      currentScene: null,
      sceneConfigs: [],
    });

    expect(screen.getAllByText("---")).toHaveLength(2);
  });

  it("shows offline mode while disconnected", () => {
    renderBottomStatusBar(disconnectedAppViewState);

    expect(screen.getByText("Offline")).toBeInTheDocument();
  });

  it("shows ready mode when connected and idle", () => {
    renderBottomStatusBar(connectedAppState);

    expect(screen.getByText("Ready")).toBeInTheDocument();
  });

  it("shows safe mode before fading when lockout is enabled", () => {
    renderBottomStatusBar({
      ...connectedAppState,
      lockout: true,
    });

    expect(screen.getByText("Safe")).toBeInTheDocument();
    expect(screen.queryByText("Fading")).not.toBeInTheDocument();
  });

  it("shows fading mode while a fade is running", () => {
    renderBottomStatusBar({ ...connectedAppState, fadeState: "running" });

    expect(screen.getByText("Fading")).toBeInTheDocument();
  });

  it("does not use the selected scene as the cued fallback", () => {
    renderBottomStatusBar({
      ...connectedAppState,
      activeCueListId: null,
      cuedCueEntryId: null,
      selectedSceneInternalId:
        connectedAppState.sceneConfigs[0].internalSceneId,
    });

    expect(screen.getByText("---")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "GO" })).toBeDisabled();
  });

  it("disables GO when the active cue list is missing", () => {
    renderBottomStatusBar({
      ...connectedAppState,
      activeCueListId: "missing-list",
    });

    expect(screen.getByText("---")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "GO" })).toBeDisabled();
  });

  it("disables GO when the cued entry is missing", () => {
    renderBottomStatusBar({
      ...connectedAppState,
      cuedCueEntryId: "missing-entry",
    });

    expect(screen.getByText("---")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "GO" })).toBeDisabled();
  });

  it("disables GO when the cued scene config is missing", () => {
    renderBottomStatusBar({
      ...connectedAppState,
      cuedCueEntryId: connectedAppState.cueLists[0].entries[0].id,
      cueLists: [
        {
          ...connectedAppState.cueLists[0],
          entries: [
            {
              ...connectedAppState.cueLists[0].entries[0],
              sceneInternalId: "missing-scene",
            },
          ],
        },
      ],
    });

    expect(screen.getByText("---")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "GO" })).toBeDisabled();
  });

  it("dispatches only one GO while recall is unresolved", async () => {
    const user = userEvent.setup();
    let resolveRecall!: () => void;
    const recallCuedCue = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveRecall = resolve;
        }),
    );

    renderBottomStatusBar(connectedAppState, { commands: { recallCuedCue } });

    const go = screen.getByRole("button", { name: "GO" });
    await user.dblClick(go);

    expect(recallCuedCue).toHaveBeenCalledTimes(1);
    expect(go).toBeDisabled();

    resolveRecall();

    await screen.findByRole("button", { name: "GO" });
    expect(go).toBeEnabled();
  });

  it("re-enables GO after a rejected recall", async () => {
    const user = userEvent.setup();
    const recallCuedCue = vi.fn(async () => {
      throw new Error("recall failed");
    });

    renderBottomStatusBar(connectedAppState, { commands: { recallCuedCue } });

    const go = screen.getByRole("button", { name: "GO" });
    await user.click(go);

    expect(recallCuedCue).toHaveBeenCalledTimes(1);
    await screen.findByRole("button", { name: "GO" });
    expect(go).toBeEnabled();
  });
});

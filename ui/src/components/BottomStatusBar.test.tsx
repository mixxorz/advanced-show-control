import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { connectedAppState } from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import { disconnectedAppViewState, type AppViewState } from "../types";
import { BottomStatusBar } from "./BottomStatusBar";

function renderBottomStatusBar(appState: AppViewState) {
  renderWithAppProviders(<BottomStatusBar appState={appState} />, { appState });
}

describe("BottomStatusBar", () => {
  it("shows dashes when no current or cued scene is available", () => {
    renderBottomStatusBar({
      ...connectedAppState,
      currentScene: null,
      cuedSceneId: null,
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

  it("shows fading mode with a pulse while a fade is running", () => {
    renderBottomStatusBar({ ...connectedAppState, fadeState: "running" });

    expect(screen.getByText("Fading")).toHaveClass("animate-pulse");
  });
});

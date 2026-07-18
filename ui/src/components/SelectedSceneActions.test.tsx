import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  connectedAppState,
  unlinkedDraftScene,
} from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import { SelectedSceneActions } from "./SelectedSceneActions";

describe("SelectedSceneActions", () => {
  it.each([
    ["linked", connectedAppState.sceneConfigs[0]],
    ["unlinked", unlinkedDraftScene],
  ])("enables Copy for a %s scene and dispatches its ID", async (_, scene) => {
    const user = userEvent.setup();
    const copySceneSettings = vi.fn();

    renderWithAppProviders(<SelectedSceneActions scene={scene} />, {
      commands: { copySceneSettings },
    });

    const copy = screen.getByRole("button", { name: "Copy" });
    expect(copy).toBeEnabled();

    await user.click(copy);

    expect(copySceneSettings).toHaveBeenCalledWith(scene.internalSceneId);
  });

  it.each([
    [
      "clipboard is unavailable",
      connectedAppState.sceneConfigs[0],
      false,
      true,
    ],
    ["destination scene is unlinked", unlinkedDraftScene, true, true],
    [
      "clipboard is available for a linked destination",
      connectedAppState.sceneConfigs[0],
      true,
      false,
    ],
  ])("disables Paste only when %s", (_, scene, available, disabled) => {
    renderWithAppProviders(<SelectedSceneActions scene={scene} />, {
      appState: {
        ...connectedAppState,
        sceneSettingsClipboardAvailable: available,
      },
    });

    const paste = screen.getByRole("button", { name: "Paste" });
    if (disabled) {
      expect(paste).toBeDisabled();
    } else {
      expect(paste).toBeEnabled();
    }
  });

  it("dispatches the selected scene ID when Paste is enabled", async () => {
    const user = userEvent.setup();
    const pasteSceneSettings = vi.fn();
    const scene = connectedAppState.sceneConfigs[0];

    renderWithAppProviders(<SelectedSceneActions scene={scene} />, {
      appState: {
        ...connectedAppState,
        sceneSettingsClipboardAvailable: true,
      },
      commands: { pasteSceneSettings },
    });

    await user.click(screen.getByRole("button", { name: "Paste" }));

    expect(pasteSceneSettings).toHaveBeenCalledWith(scene.internalSceneId);
  });
});

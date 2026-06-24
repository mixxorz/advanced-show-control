import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderWithAppProviders } from "../test/render";
import { disconnectedAppViewState } from "../types";
import { SettingsTab } from "./SettingsTab";

const replaceAppSettings = vi.fn();

vi.mock("../commands", async (actual) => ({
  ...(await actual<typeof import("../commands")>()),
  replaceAppSettings: (settings: unknown) => replaceAppSettings(settings),
}));

describe("SettingsTab", () => {
  it("renders projected settings and replaces the full object on toggle", () => {
    const state = {
      ...disconnectedAppViewState,
      settings: {
        autoLoadLastShowFile: false,
        autoSaveSessions: false,
        keyboardShortcuts: {
          go: {
            key: "Space",
            modifiers: {
              shift: false,
              control: false,
              alt: false,
              meta: false,
            },
          },
          cue: {
            key: "C",
            modifiers: {
              shift: false,
              control: false,
              alt: false,
              meta: false,
            },
          },
        },
        autoCueNextSceneOnGo: false,
        timeDisplay: "twentyFourHour" as const,
        faderOverrideSensitivity: 9,
      },
    };

    renderWithAppProviders(<SettingsTab />, { appState: state });
    fireEvent.click(screen.getByLabelText("Auto save sessions"));

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...state.settings,
      autoSaveSessions: true,
    });
  });

  it("sends sensitivity updates as a bounded number", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.change(screen.getByLabelText("Fader override sensitivity"), {
      target: { value: "10" },
    });

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      faderOverrideSensitivity: 10,
    });
  });
});

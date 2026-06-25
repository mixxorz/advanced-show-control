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

  it("updates auto-load while replacing the full settings object", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(screen.getByLabelText("Auto load last show file"));

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      autoLoadLastShowFile: true,
    });
  });

  it("updates auto-cue while replacing the full settings object", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(screen.getByLabelText("Auto cue next scene on GO"));

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      autoCueNextSceneOnGo: true,
    });
  });

  it("updates time display while replacing the full settings object", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.change(screen.getByLabelText("Time display"), {
      target: { value: "twelveHour" },
    });

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      timeDisplay: "twelveHour",
    });
  });

  it("updates the GO shortcut key while replacing the full settings object", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.change(screen.getByLabelText("GO keyboard shortcut"), {
      target: { value: "Enter" },
    });

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      keyboardShortcuts: {
        ...disconnectedAppViewState.settings.keyboardShortcuts,
        go: {
          ...disconnectedAppViewState.settings.keyboardShortcuts.go,
          key: "Enter",
        },
      },
    });
  });

  it("updates the Cue shortcut key while replacing the full settings object", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.change(screen.getByLabelText("Cue keyboard shortcut"), {
      target: { value: "Q" },
    });

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      keyboardShortcuts: {
        ...disconnectedAppViewState.settings.keyboardShortcuts,
        cue: {
          ...disconnectedAppViewState.settings.keyboardShortcuts.cue,
          key: "Q",
        },
      },
    });
  });

  it("updates the GO shortcut modifier while replacing the full settings object", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(screen.getByLabelText("GO Shift"));

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      keyboardShortcuts: {
        ...disconnectedAppViewState.settings.keyboardShortcuts,
        go: {
          ...disconnectedAppViewState.settings.keyboardShortcuts.go,
          modifiers: {
            ...disconnectedAppViewState.settings.keyboardShortcuts.go.modifiers,
            shift: true,
          },
        },
      },
    });
  });

  it("updates the Cue shortcut modifier while replacing the full settings object", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(screen.getByLabelText("Cue Control"));

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      keyboardShortcuts: {
        ...disconnectedAppViewState.settings.keyboardShortcuts,
        cue: {
          ...disconnectedAppViewState.settings.keyboardShortcuts.cue,
          modifiers: {
            ...disconnectedAppViewState.settings.keyboardShortcuts.cue
              .modifiers,
            control: true,
          },
        },
      },
    });
  });
});

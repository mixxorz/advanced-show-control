import { fireEvent, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithAppProviders } from "../test/render";
import { disconnectedAppViewState } from "../types";
import { SettingsTab } from "./SettingsTab";

const replaceAppSettings = vi.fn();

vi.mock("../commands", async (actual) => ({
  ...(await actual<typeof import("../commands")>()),
  replaceAppSettings: (settings: unknown) => replaceAppSettings(settings),
}));

describe("SettingsTab", () => {
  beforeEach(() => {
    replaceAppSettings.mockReset();
  });

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

  it("captures the GO shortcut while replacing the full settings object", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(
      screen.getByRole("button", { name: "Change GO keyboard shortcut" }),
    );
    expect(screen.getByText("Press shortcut...")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "Enter", shiftKey: true });

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      keyboardShortcuts: {
        ...disconnectedAppViewState.settings.keyboardShortcuts,
        go: {
          key: "Enter",
          modifiers: {
            shift: true,
            control: false,
            alt: false,
            meta: false,
          },
        },
      },
    });
  });

  it("captures the Cue shortcut while replacing the full settings object", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(
      screen.getByRole("button", { name: "Change Cue keyboard shortcut" }),
    );
    fireEvent.keyDown(window, { key: "q", ctrlKey: true });

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      keyboardShortcuts: {
        ...disconnectedAppViewState.settings.keyboardShortcuts,
        cue: {
          key: "Q",
          modifiers: {
            shift: false,
            control: true,
            alt: false,
            meta: false,
          },
        },
      },
    });
  });

  it("does not save a shortcut for modifier-only keydown", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(
      screen.getByRole("button", { name: "Change GO keyboard shortcut" }),
    );
    fireEvent.keyDown(window, { key: "Shift", shiftKey: true });

    expect(replaceAppSettings).not.toHaveBeenCalled();
    expect(screen.getByText("Press shortcut...")).toBeInTheDocument();
  });

  it("cancels shortcut capture on Escape", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(
      screen.getByRole("button", { name: "Change GO keyboard shortcut" }),
    );
    fireEvent.keyDown(window, { key: "Escape" });

    expect(replaceAppSettings).not.toHaveBeenCalled();
    expect(screen.queryByText("Press shortcut...")).not.toBeInTheDocument();
  });

  it("captures Tab as a shortcut", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(
      screen.getByRole("button", { name: "Change GO keyboard shortcut" }),
    );
    fireEvent.keyDown(window, { key: "Tab" });

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      keyboardShortcuts: {
        ...disconnectedAppViewState.settings.keyboardShortcuts,
        go: {
          key: "Tab",
          modifiers: {
            shift: false,
            control: false,
            alt: false,
            meta: false,
          },
        },
      },
    });
  });
});

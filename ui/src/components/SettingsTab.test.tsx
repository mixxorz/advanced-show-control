import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithAppProviders } from "../test/render";
import { disconnectedAppViewState } from "../types";
import { MockAppProviders } from "../storybook/MockAppProviders";
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

    fireEvent.click(
      screen.getByRole("button", {
        name: "Increase Fader override sensitivity",
      }),
    );

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      faderOverrideSensitivity:
        disconnectedAppViewState.settings.faderOverrideSensitivity + 1,
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

  it("composes rapid full-object setting updates before projection refreshes", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(screen.getByLabelText("Auto load last show file"));
    fireEvent.click(screen.getByLabelText("Auto save sessions"));

    expect(replaceAppSettings).toHaveBeenLastCalledWith({
      ...disconnectedAppViewState.settings,
      autoLoadLastShowFile: true,
      autoSaveSessions: true,
    });
  });

  it("keeps composing draft settings across unrelated projection updates", () => {
    const { rerender } = render(
      <MockAppProviders appState={disconnectedAppViewState}>
        <SettingsTab />
      </MockAppProviders>,
    );

    fireEvent.click(screen.getByLabelText("Auto load last show file"));

    rerender(
      <MockAppProviders
        appState={{
          ...disconnectedAppViewState,
          stateVersion: disconnectedAppViewState.stateVersion + 1,
        }}
      >
        <SettingsTab />
      </MockAppProviders>,
    );

    fireEvent.click(screen.getByLabelText("Auto save sessions"));

    expect(replaceAppSettings).toHaveBeenLastCalledWith({
      ...disconnectedAppViewState.settings,
      autoLoadLastShowFile: true,
      autoSaveSessions: true,
    });
  });

  it("keeps the latest draft visible across intermediate settings projections", () => {
    const { rerender } = render(
      <MockAppProviders appState={disconnectedAppViewState}>
        <SettingsTab />
      </MockAppProviders>,
    );

    fireEvent.click(screen.getByLabelText("Auto load last show file"));
    fireEvent.click(screen.getByLabelText("Auto save sessions"));

    rerender(
      <MockAppProviders
        appState={{
          ...disconnectedAppViewState,
          stateVersion: disconnectedAppViewState.stateVersion + 1,
          settings: {
            ...disconnectedAppViewState.settings,
            autoLoadLastShowFile: true,
          },
        }}
      >
        <SettingsTab />
      </MockAppProviders>,
    );

    expect(screen.getByLabelText("Auto load last show file")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByLabelText("Auto save sessions")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("shows a settings save error when replacement fails", async () => {
    replaceAppSettings.mockRejectedValueOnce(new Error("settings disk full"));
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(screen.getByLabelText("Auto load last show file"));

    expect(
      await screen.findByText("Error: settings disk full"),
    ).toBeInTheDocument();
  });

  it("clears a previous settings save error after a later successful replacement", async () => {
    replaceAppSettings
      .mockRejectedValueOnce(new Error("settings disk full"))
      .mockResolvedValueOnce(undefined);
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(screen.getByLabelText("Auto load last show file"));
    expect(
      await screen.findByText("Error: settings disk full"),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByLabelText("Auto save sessions"));

    await waitFor(() => {
      expect(
        screen.queryByText("Error: settings disk full"),
      ).not.toBeInTheDocument();
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
    expect(screen.getByText("...")).toBeInTheDocument();

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
    expect(screen.getByText("...")).toBeInTheDocument();
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
    expect(screen.queryByText("...")).not.toBeInTheDocument();
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

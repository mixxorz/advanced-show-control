import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithAppProviders } from "../test/render";
import { disconnectedAppViewState } from "../types";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { createDeferred } from "../test/deferred";
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

  it("updates auto-save by replacing the full settings object", () => {
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
        timeDisplay: "twentyFourHour" as const,
        faderOverrideSensitivity: 9,
        enableExtensiveDiagnostics: false,
        sameSceneRecallEnabled: true,
        sameSceneRecallThresholdMs: 500,
      },
    };

    renderWithAppProviders(<SettingsTab />, { appState: state });
    fireEvent.click(screen.getByLabelText("Auto save sessions"));

    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...state.settings,
      autoSaveSessions: true,
    });
  });

  it.each([
    {
      control: "Increase Fader override sensitivity",
      expected: { faderOverrideSensitivity: 10 },
    },
    {
      control: "Auto load last show file",
      expected: { autoLoadLastShowFile: true },
    },
    {
      control: "Extensive diagnostics",
      expected: { enableExtensiveDiagnostics: true },
    },
    {
      control: "Same scene recall finishing",
      expected: { sameSceneRecallEnabled: false },
    },
    {
      control: "Increase Same scene recall threshold",
      expected: { sameSceneRecallThresholdMs: 600 },
    },
  ])(
    "replaces the full settings object when using $control",
    ({ control, expected }) => {
      renderWithAppProviders(<SettingsTab />, {
        appState: disconnectedAppViewState,
      });

      fireEvent.click(screen.getByRole("button", { name: control }));

      expect(replaceAppSettings).toHaveBeenCalledWith({
        ...disconnectedAppViewState.settings,
        ...expected,
      });
    },
  );

  it.each([
    [0, "Decrease Same scene recall threshold"],
    [5000, "Increase Same scene recall threshold"],
  ] as const)(
    "keeps same-scene threshold %i within bounds",
    (value, buttonName) => {
      renderWithAppProviders(<SettingsTab />, {
        appState: {
          ...disconnectedAppViewState,
          settings: {
            ...disconnectedAppViewState.settings,
            sameSceneRecallEnabled: false,
            sameSceneRecallThresholdMs: value,
          },
        },
      });

      fireEvent.click(screen.getByRole("button", { name: buttonName }));
      expect(replaceAppSettings).toHaveBeenCalledWith({
        ...disconnectedAppViewState.settings,
        sameSceneRecallEnabled: false,
        sameSceneRecallThresholdMs: value,
      });
    },
  );

  it("composes rapid full-object setting updates before projection refreshes", async () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(screen.getByLabelText("Auto load last show file"));
    fireEvent.click(screen.getByLabelText("Auto save sessions"));

    await waitFor(() =>
      expect(replaceAppSettings).toHaveBeenLastCalledWith({
        ...disconnectedAppViewState.settings,
        autoLoadLastShowFile: true,
        autoSaveSessions: true,
      }),
    );
  });

  it("keeps composing draft settings across unrelated projection updates", async () => {
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

    await waitFor(() =>
      expect(replaceAppSettings).toHaveBeenLastCalledWith({
        ...disconnectedAppViewState.settings,
        autoLoadLastShowFile: true,
        autoSaveSessions: true,
      }),
    );
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

  it("does not restore an acknowledged draft after a later authoritative update", () => {
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
          settings: {
            sameSceneRecallThresholdMs:
              disconnectedAppViewState.settings.sameSceneRecallThresholdMs,
            sameSceneRecallEnabled:
              disconnectedAppViewState.settings.sameSceneRecallEnabled,
            enableExtensiveDiagnostics:
              disconnectedAppViewState.settings.enableExtensiveDiagnostics,
            faderOverrideSensitivity:
              disconnectedAppViewState.settings.faderOverrideSensitivity,
            timeDisplay: disconnectedAppViewState.settings.timeDisplay,
            keyboardShortcuts: {
              cue: disconnectedAppViewState.settings.keyboardShortcuts.cue,
              go: disconnectedAppViewState.settings.keyboardShortcuts.go,
            },
            autoSaveSessions:
              disconnectedAppViewState.settings.autoSaveSessions,
            autoLoadLastShowFile: true,
          },
        }}
      >
        <SettingsTab />
      </MockAppProviders>,
    );

    rerender(
      <MockAppProviders
        appState={{
          ...disconnectedAppViewState,
          stateVersion: disconnectedAppViewState.stateVersion + 2,
        }}
      >
        <SettingsTab />
      </MockAppProviders>,
    );

    expect(screen.getByLabelText("Auto load last show file")).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("catches a synchronous replacement failure and clears the latest draft", async () => {
    renderWithAppProviders(
      <SettingsTab
        onReplaceSettings={() => {
          throw new Error("synchronous settings failure");
        }}
      />,
      { appState: disconnectedAppViewState },
    );

    const autoLoad = screen.getByLabelText("Auto load last show file");
    fireEvent.click(autoLoad);

    expect(
      await screen.findByText("Error: synchronous settings failure"),
    ).toBeInTheDocument();
    expect(autoLoad).toHaveAttribute("aria-pressed", "false");
  });

  it("serializes rapid replacements while composing the latest draft immediately", async () => {
    const first = createDeferred<void>();
    const second = createDeferred<void>();
    const onReplaceSettings = vi
      .fn()
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => second.promise);

    renderWithAppProviders(
      <SettingsTab onReplaceSettings={onReplaceSettings} />,
      { appState: disconnectedAppViewState },
    );

    fireEvent.click(screen.getByLabelText("Auto load last show file"));
    fireEvent.click(screen.getByLabelText("Auto save sessions"));

    expect(screen.getByLabelText("Auto load last show file")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByLabelText("Auto save sessions")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(onReplaceSettings).toHaveBeenCalledTimes(1);

    await act(async () => {
      first.reject(new Error("superseded failure"));
      await first.promise.catch(() => undefined);
    });

    await waitFor(() => expect(onReplaceSettings).toHaveBeenCalledTimes(2));
    expect(onReplaceSettings).toHaveBeenLastCalledWith({
      ...disconnectedAppViewState.settings,
      autoLoadLastShowFile: true,
      autoSaveSessions: true,
    });
    expect(
      screen.queryByText("Error: superseded failure"),
    ).not.toBeInTheDocument();
    expect(screen.getByLabelText("Auto save sessions")).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    await act(async () => {
      second.resolve();
      await second.promise;
    });
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

  it("releases shortcut capture when navigating away from Settings", () => {
    const { rerender } = renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(
      screen.getByRole("button", { name: "Change GO keyboard shortcut" }),
    );
    rerender(<div>Navigated away</div>);

    const keydown = new KeyboardEvent("keydown", {
      key: "Enter",
      bubbles: true,
      cancelable: true,
    });
    window.dispatchEvent(keydown);

    expect(keydown.defaultPrevented).toBe(false);
    expect(replaceAppSettings).not.toHaveBeenCalled();
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

  it("rejects and accessibly describes a shortcut assigned to the other action", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(
      screen.getByRole("button", { name: "Change GO keyboard shortcut" }),
    );
    fireEvent.keyDown(window, { key: "c", code: "KeyC" });

    expect(replaceAppSettings).not.toHaveBeenCalled();
    const conflict = screen.getByRole("alert");
    expect(conflict).toHaveTextContent("Already assigned to Cue");
    expect(
      screen.getByRole("button", { name: "Change GO keyboard shortcut" }),
    ).toHaveAttribute("aria-describedby", conflict.id);
  });

  it("rejects a captured shortcut that differs from Cue only by key case", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: {
        ...disconnectedAppViewState,
        settings: {
          ...disconnectedAppViewState.settings,
          keyboardShortcuts: {
            ...disconnectedAppViewState.settings.keyboardShortcuts,
            cue: {
              ...disconnectedAppViewState.settings.keyboardShortcuts.cue,
              key: "c",
            },
          },
        },
      },
    });

    fireEvent.click(
      screen.getByRole("button", { name: "Change GO keyboard shortcut" }),
    );
    fireEvent.keyDown(window, { key: "C", code: "KeyC" });

    expect(replaceAppSettings).not.toHaveBeenCalled();
    expect(screen.getByText("Already assigned to Cue")).toBeInTheDocument();
  });

  it("rejects a captured shortcut reserved by a fixed File menu accelerator", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(
      screen.getByRole("button", { name: "Change Cue keyboard shortcut" }),
    );
    fireEvent.keyDown(window, { key: "s", code: "KeyS", ctrlKey: true });

    expect(replaceAppSettings).not.toHaveBeenCalled();
    expect(
      screen.getByText("Already assigned to Save Session"),
    ).toBeInTheDocument();
  });

  it("clears shortcut conflict text after a successful non-conflicting capture", () => {
    renderWithAppProviders(<SettingsTab />, {
      appState: disconnectedAppViewState,
    });

    fireEvent.click(
      screen.getByRole("button", { name: "Change GO keyboard shortcut" }),
    );
    fireEvent.keyDown(window, { key: "c", code: "KeyC" });
    expect(screen.getByText("Already assigned to Cue")).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: "Change GO keyboard shortcut" }),
    );
    fireEvent.keyDown(window, { key: "Enter", code: "Enter", shiftKey: true });

    expect(
      screen.queryByText("Already assigned to Cue"),
    ).not.toBeInTheDocument();
    expect(replaceAppSettings).toHaveBeenCalledWith({
      ...disconnectedAppViewState.settings,
      keyboardShortcuts: {
        ...disconnectedAppViewState.settings.keyboardShortcuts,
        go: {
          key: "Enter",
          modifiers: { shift: true, control: false, alt: false, meta: false },
        },
      },
    });
  });
});

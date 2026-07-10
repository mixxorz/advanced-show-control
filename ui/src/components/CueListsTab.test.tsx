import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  cueListStateFixture,
  cueListWithMissingSceneReferenceAppState,
} from "../storybook/mockAppState";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { renderWithAppProviders } from "../test/render";
import { CueListsTab } from "./CueListsTab";

describe("CueListsTab", () => {
  it("renders the scene library and active cue list", () => {
    renderWithAppProviders(<CueListsTab />, { appState: cueListStateFixture });

    expect(
      screen.getByRole("heading", { name: /Scene library/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Main" })).toBeInTheDocument();
    expect(screen.getAllByText("Scene Name")).toHaveLength(2);
    expect(screen.getAllByText("#")).toHaveLength(2);
    expect(
      screen.getByRole("button", { name: /Intro.*001/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Main.*002/i }),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText(/Active cue list/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Cued: Cue 1: Intro/i)).not.toBeInTheDocument();
  });

  it("selects a cue entry before cueing it", async () => {
    const user = userEvent.setup();
    const cueEntry = vi.fn();

    renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands: { cueEntry },
    });

    const cueButton = screen.getByRole("button", { name: "Cue" });
    expect(cueButton).toBeDisabled();

    await user.click(screen.getByRole("button", { name: /Main.*002/i }));
    expect(cueEntry).not.toHaveBeenCalled();
    expect(cueButton).toBeEnabled();

    await user.click(cueButton);
    expect(cueEntry).toHaveBeenCalledWith("cue-2");
    expect(cueButton).toBeDisabled();
  });

  it("cues the selected cue-list entry with the configured Cue shortcut", async () => {
    const user = userEvent.setup();
    const cueEntry = vi.fn();
    renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands: { cueEntry },
    });

    await user.click(screen.getByRole("button", { name: /Main.*002/i }));
    act(() => {
      window.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "c",
          code: "KeyC",
          bubbles: true,
          cancelable: true,
        }),
      );
    });

    expect(cueEntry).toHaveBeenCalledWith("cue-2");
    expect(screen.getByRole("button", { name: "Cue" })).toBeDisabled();
  });

  it("does not cue the selected entry from an editable dialog field", async () => {
    const user = userEvent.setup();
    const cueEntry = vi.fn();
    renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands: { cueEntry },
    });

    await user.click(screen.getByRole("button", { name: /Main.*002/i }));
    await user.click(screen.getByRole("button", { name: "Manage Cue Lists" }));
    await user.click(screen.getByRole("button", { name: "New Cue List" }));
    const input = screen.getByLabelText("Cue list name");
    const event = new KeyboardEvent("keydown", {
      key: "c",
      code: "KeyC",
      bubbles: true,
      cancelable: true,
    });

    act(() => input.dispatchEvent(event));

    expect(cueEntry).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it("consumes repeated Cue keydowns without cueing the selected entry", async () => {
    const user = userEvent.setup();
    const cueEntry = vi.fn();
    renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands: { cueEntry },
    });

    await user.click(screen.getByRole("button", { name: /Main.*002/i }));
    const event = new KeyboardEvent("keydown", {
      key: "c",
      code: "KeyC",
      bubbles: true,
      cancelable: true,
      repeat: true,
    });
    act(() => window.dispatchEvent(event));

    expect(cueEntry).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(true);
  });

  it("does not run the Cue shortcut without a selected entry", () => {
    const cueEntry = vi.fn();
    renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands: { cueEntry },
    });

    act(() => {
      window.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "c",
          code: "KeyC",
          bubbles: true,
          cancelable: true,
        }),
      );
    });

    expect(cueEntry).not.toHaveBeenCalled();
  });

  it("disables Cue when the selected entry disappears", async () => {
    const user = userEvent.setup();
    const appState = {
      ...cueListStateFixture,
      cueLists: cueListStateFixture.cueLists.map((cueList) =>
        cueList.id === "cue-list-main"
          ? {
              ...cueList,
              entries: cueList.entries.filter((entry) => entry.id !== "cue-2"),
            }
          : cueList,
      ),
    };
    const { rerender } = renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
    });

    await user.click(screen.getByRole("button", { name: /Main.*002/i }));
    expect(screen.getByRole("button", { name: "Cue" })).toBeEnabled();

    rerender(
      <MockAppProviders appState={appState}>
        <CueListsTab />
      </MockAppProviders>,
    );

    expect(screen.getByRole("button", { name: "Cue" })).toBeDisabled();
  });

  it("disables Cue when the active list changes", async () => {
    const user = userEvent.setup();
    const { rerender } = renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
    });

    await user.click(screen.getByRole("button", { name: /Main.*002/i }));
    expect(screen.getByRole("button", { name: "Cue" })).toBeEnabled();

    rerender(
      <MockAppProviders
        appState={{
          ...cueListStateFixture,
          activeCueListId: "cue-list-verse",
          cuedCueEntryId: null,
        }}
      >
        <CueListsTab />
      </MockAppProviders>,
    );

    expect(screen.getByRole("button", { name: "Cue" })).toBeDisabled();
  });

  it("cues a cue entry directly on double click", async () => {
    const user = userEvent.setup();
    const cueEntry = vi.fn();

    renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands: { cueEntry },
    });

    const cueButton = screen.getByRole("button", { name: "Cue" });

    await user.dblClick(screen.getByRole("button", { name: /Main.*002/i }));

    expect(cueEntry).toHaveBeenCalledWith("cue-2");
    expect(cueButton).toBeDisabled();
  });

  it("opens cue list management from the panel header", async () => {
    const user = userEvent.setup();

    renderWithAppProviders(<CueListsTab />, { appState: cueListStateFixture });

    await user.click(screen.getByRole("button", { name: /Manage Cue Lists/i }));

    expect(
      screen.getByRole("dialog", { name: /Manage Cue Lists/i }),
    ).toBeInTheDocument();
  });

  it("does not cue a stale selected entry after the active list changes", async () => {
    const user = userEvent.setup();
    const cueEntry = vi.fn();
    const { rerender } = renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands: { cueEntry },
    });

    await user.click(screen.getByRole("button", { name: /Main.*002/i }));

    rerender(
      <MockAppProviders
        appState={{
          ...cueListStateFixture,
          activeCueListId: "cue-list-verse",
          cuedCueEntryId: null,
        }}
      >
        <CueListsTab />
      </MockAppProviders>,
    );

    await user.click(screen.getByRole("button", { name: "Cue" }));

    expect(cueEntry).not.toHaveBeenCalled();
  });

  it("clears the local cue selection when it leaves the active list and keeps Cue disabled after returning", async () => {
    const user = userEvent.setup();
    const cueEntry = vi.fn();
    const { rerender } = renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands: { cueEntry },
    });

    await user.click(screen.getByRole("button", { name: /Main.*002/i }));
    expect(screen.getByRole("button", { name: "Cue" })).toBeEnabled();

    rerender(
      <MockAppProviders
        appState={{
          ...cueListStateFixture,
          activeCueListId: "cue-list-verse",
          cuedCueEntryId: null,
        }}
      >
        <CueListsTab />
      </MockAppProviders>,
    );

    expect(screen.getByRole("button", { name: "Cue" })).toBeDisabled();

    rerender(
      <MockAppProviders appState={cueListStateFixture}>
        <CueListsTab />
      </MockAppProviders>,
    );

    expect(screen.getByRole("button", { name: "Cue" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: /Main.*002/i }));
    expect(screen.getByRole("button", { name: "Cue" })).toBeEnabled();
  });

  it("cancels stale queued selection cleanup after projection changes", async () => {
    const user = userEvent.setup();
    const queuedCallbacks: Array<() => void> = [];
    const queueMicrotaskSpy = vi
      .spyOn(globalThis, "queueMicrotask")
      .mockImplementation((callback) => {
        queuedCallbacks.push(callback);
      });
    const { rerender } = renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
    });

    await user.click(screen.getByRole("button", { name: /Main.*002/i }));
    rerender(
      <MockAppProviders
        appState={{
          ...cueListStateFixture,
          activeCueListId: "cue-list-verse",
          cuedCueEntryId: null,
        }}
      >
        <CueListsTab />
      </MockAppProviders>,
    );
    rerender(
      <MockAppProviders appState={cueListStateFixture}>
        <CueListsTab />
      </MockAppProviders>,
    );

    await user.click(screen.getByRole("button", { name: /Intro.*001/i }));
    expect(screen.getByRole("button", { name: "Cue" })).toBeEnabled();

    queuedCallbacks.forEach((callback) => callback());

    expect(screen.getByRole("button", { name: "Cue" })).toBeEnabled();
    queueMicrotaskSpy.mockRestore();
  });

  it("marks cue entries with missing scene references", () => {
    renderWithAppProviders(<CueListsTab />, {
      appState: cueListWithMissingSceneReferenceAppState,
    });

    expect(
      screen.getByRole("button", { name: /Missing scene.*---/i }),
    ).toBeInTheDocument();
  });

  it("removes cue entries immediately without opening a confirmation modal", async () => {
    const user = userEvent.setup();
    const removeCueEntry = vi.fn();

    renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands: { removeCueEntry },
    });

    await user.click(screen.getByRole("button", { name: /Remove cue 1/i }));

    expect(removeCueEntry).toHaveBeenCalledWith("cue-1");
    expect(
      screen.queryByRole("dialog", { name: /Delete Cue/i }),
    ).not.toBeInTheDocument();
  });
});

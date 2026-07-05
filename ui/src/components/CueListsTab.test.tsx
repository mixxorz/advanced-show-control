import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  cueListStateFixture,
  cueListWithMissingSceneReferenceAppState,
} from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import { CueListsTab } from "./CueListsTab";

describe("CueListsTab", () => {
  it("renders the scene library and active cue list selector", () => {
    renderWithAppProviders(<CueListsTab />, { appState: cueListStateFixture });

    expect(
      screen.getByRole("heading", { name: /Scene Library/i }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(/Active cue list/i)).toHaveValue(
      "cue-list-main",
    );
    expect(screen.getByText("Intro")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Cue 1: Intro" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Cued: Cue 1: Intro")).toBeInTheDocument();
    expect(screen.getByText("Next: Cue 2: Main")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /^Intro$/i }),
    ).not.toBeInTheDocument();
  });

  it("shows next cue relative to the currently cued entry", () => {
    renderWithAppProviders(<CueListsTab />, {
      appState: {
        ...cueListStateFixture,
        cuedCueEntryId: "cue-2",
        cueLists: [
          {
            ...cueListStateFixture.cueLists[0],
            entries: [
              ...cueListStateFixture.cueLists[0].entries,
              { id: "cue-3", sceneInternalId: "scene-intro" },
            ],
          },
          ...cueListStateFixture.cueLists.slice(1),
        ],
      },
    });

    expect(screen.getByText("Cued: Cue 2: Main")).toBeInTheDocument();
    expect(screen.getByText("Next: Cue 3: Intro")).toBeInTheDocument();
  });

  it("marks cue entries with missing scene references", () => {
    renderWithAppProviders(<CueListsTab />, {
      appState: cueListWithMissingSceneReferenceAppState,
    });

    expect(
      screen.getByRole("button", { name: "Cue 2: Missing scene" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Cued: Cue 2: Missing scene")).toBeInTheDocument();
  });

  it("adds a scene by dragging it onto a cue-list drop zone", () => {
    const addSceneToActiveCueList = vi.fn();

    renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands: { addSceneToActiveCueList },
    });

    fireEvent.dragStart(screen.getByText("Intro"), {
      dataTransfer: {
        setData: vi.fn(),
      },
    });
    fireEvent.drop(screen.getByLabelText(/Drop scene at position 2/i), {
      dataTransfer: {
        getData: () => "scene-intro",
      },
    });

    expect(addSceneToActiveCueList).toHaveBeenCalledWith("scene-intro", 1);
  });

  it("switches the active cue list from the header selector", async () => {
    const user = userEvent.setup();
    const setActiveCueList = vi.fn();

    renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands: { setActiveCueList },
    });

    await user.selectOptions(
      screen.getByLabelText(/Active cue list/i),
      "cue-list-verse",
    );

    expect(setActiveCueList).toHaveBeenCalledWith("cue-list-verse");
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

  it("moves the active cue list with the reorder controls", async () => {
    const user = userEvent.setup();
    const reorderCueLists = vi.fn();

    renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands: { reorderCueLists },
    });

    await user.click(screen.getByRole("button", { name: /Move Down/i }));

    expect(reorderCueLists).toHaveBeenCalledWith([
      "cue-list-verse",
      "cue-list-main",
    ]);
  });
});

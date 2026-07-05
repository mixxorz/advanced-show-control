import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { cueListStateFixture } from "../storybook/mockAppState";
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
  });

  it("adds a scene by dragging it onto a cue-list drop zone", () => {
    const addSceneToActiveCueList = vi.fn();

    renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands: { addSceneToActiveCueList },
    });

    fireEvent.dragStart(screen.getByRole("button", { name: /Intro/i }), {
      dataTransfer: {
        setData: vi.fn(),
      },
    });
    fireEvent.drop(
      screen.getByRole("button", { name: /Drop scene at position 2/i }),
      {
        dataTransfer: {
          getData: () => "scene-intro",
        },
      },
    );

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

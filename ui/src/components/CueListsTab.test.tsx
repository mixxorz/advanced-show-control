import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  cueListStateFixture,
  cueListWithMissingSceneReferenceAppState,
} from "../storybook/mockAppState";
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

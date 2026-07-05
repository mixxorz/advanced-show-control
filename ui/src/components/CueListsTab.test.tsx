import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { cueListStateFixture } from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import { CueListsTab } from "./CueListsTab";

describe("CueListsTab", () => {
  it("renders scene library on the left and active cue list in the main pane", () => {
    renderWithAppProviders(<CueListsTab />, { appState: cueListStateFixture });

    expect(
      screen.getByRole("heading", { name: /Scene Library/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: /Cue List/i }),
    ).toBeInTheDocument();
    expect(screen.getByText("Intro")).toBeInTheDocument();
  });

  it("opens the shared new cue list name modal from the main controls", async () => {
    const user = userEvent.setup();
    renderWithAppProviders(<CueListsTab />, { appState: cueListStateFixture });

    await user.click(screen.getByRole("button", { name: /New Cue List/i }));

    expect(
      screen.getByRole("dialog", { name: /New Cue List/i }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(/Cue list name/i)).toBeInTheDocument();
  });

  it("requires confirmation before deleting a cue list but not a cue entry", async () => {
    const user = userEvent.setup();
    const commands = {
      removeCueEntry: vi.fn(),
    };

    renderWithAppProviders(<CueListsTab />, {
      appState: cueListStateFixture,
      commands,
    });

    await user.click(screen.getByRole("button", { name: /Manage Cue Lists/i }));
    await user.click(screen.getByRole("button", { name: /Delete Main/i }));
    expect(
      screen.getByRole("dialog", { name: /Delete Cue List/i }),
    ).toBeInTheDocument();

    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: /Remove cue 1/i }));
    expect(commands.removeCueEntry).toHaveBeenCalled();
    expect(
      screen.queryByRole("dialog", { name: /Delete Cue/i }),
    ).not.toBeInTheDocument();
  });
});

import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { cueListStateFixture } from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import { CueListManageModal } from "./CueListManageModal";

describe("CueListManageModal", () => {
  it("supports activate, create, rename, and delete flows", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const commands = {
      createCueList: vi.fn(),
      renameCueList: vi.fn(),
      deleteCueList: vi.fn(),
      setActiveCueList: vi.fn(),
    };

    renderWithAppProviders(<CueListManageModal onClose={onClose} />, {
      appState: cueListStateFixture,
      commands,
    });

    await user.click(screen.getByRole("button", { name: /^Verse$/i }));
    expect(commands.setActiveCueList).toHaveBeenCalledWith("cue-list-verse");
    expect(onClose).toHaveBeenCalledTimes(1);

    commands.setActiveCueList.mockClear();
    onClose.mockClear();

    await user.click(screen.getByRole("button", { name: /^Main$/i }));
    expect(commands.setActiveCueList).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: /New Cue List/i }));
    await user.type(screen.getByLabelText(/Cue list name/i), "Bridge");
    await user.click(screen.getByRole("button", { name: /Create/i }));
    expect(commands.createCueList).toHaveBeenCalledWith("Bridge");

    await user.click(screen.getAllByRole("button", { name: /Rename/i })[0]);
    const renameDialog = screen.getByRole("dialog", {
      name: /Rename Cue List/i,
    });
    expect(renameDialog).toBeInTheDocument();
    await user.clear(within(renameDialog).getByLabelText(/Cue list name/i));
    await user.type(
      within(renameDialog).getByLabelText(/Cue list name/i),
      "Main Set",
    );
    await user.click(
      within(renameDialog).getByRole("button", { name: /Rename/i }),
    );
    expect(commands.renameCueList).toHaveBeenCalledWith(
      "cue-list-main",
      "Main Set",
    );

    await user.click(screen.getAllByRole("button", { name: /Delete/i })[0]);
    const deleteDialog = screen.getByRole("dialog", {
      name: /Delete Cue List/i,
    });
    expect(deleteDialog).toBeInTheDocument();
    await user.click(
      within(deleteDialog).getByRole("button", { name: /Delete/i }),
    );
    expect(commands.deleteCueList).toHaveBeenCalledWith("cue-list-main");

    await user.click(
      screen.getByRole("button", { name: /Close manage cue lists modal/i }),
    );
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});

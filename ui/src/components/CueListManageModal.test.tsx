import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { cueListStateFixture } from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import { CueListManageModal } from "./CueListManageModal";

describe("CueListManageModal", () => {
  it("keeps the name dialog open until creation succeeds", async () => {
    const user = userEvent.setup();
    let resolveCreate!: () => void;
    const createCueList = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveCreate = resolve;
        }),
    );

    renderWithAppProviders(<CueListManageModal onClose={vi.fn()} />, {
      appState: cueListStateFixture,
      commands: { createCueList },
    });

    await user.click(screen.getByRole("button", { name: "New Cue List" }));
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(screen.getByRole("dialog", { name: "New Cue List" })).toBeVisible();

    resolveCreate();
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "New Cue List" }),
      ).not.toBeInTheDocument(),
    );
  });

  it("makes management controls inert whenever a nested name dialog is open", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const setActiveCueList = vi.fn();

    renderWithAppProviders(<CueListManageModal onClose={onClose} />, {
      appState: cueListStateFixture,
      commands: { setActiveCueList },
    });

    await user.click(screen.getByRole("button", { name: "Rename Main" }));

    const manageDialog = screen.getByRole("dialog", {
      name: "Manage Cue Lists",
    });
    expect(manageDialog).toHaveAttribute("inert");
    expect(
      within(manageDialog).getByRole("button", {
        name: "Close manage cue lists modal",
      }),
    ).toBeDisabled();
    expect(
      within(manageDialog).getByRole("button", { name: "Verse" }),
    ).toBeDisabled();
    expect(
      within(manageDialog).getByRole("button", { name: "Rename Verse" }),
    ).toBeDisabled();

    await user.click(
      within(manageDialog).getByRole("button", {
        name: "Close manage cue lists modal",
      }),
    );
    await user.click(
      within(manageDialog).getByRole("button", { name: "Verse" }),
    );
    await user.click(
      within(manageDialog).getByRole("button", { name: "Rename Verse" }),
    );

    expect(onClose).not.toHaveBeenCalled();
    expect(setActiveCueList).not.toHaveBeenCalled();
    expect(
      screen.getByRole("dialog", { name: "Rename Cue List" }),
    ).toBeVisible();
    expect(screen.getByLabelText("Cue list name")).toHaveValue("Main");
  });

  it("keeps outer controls inert while a nested name submission is pending", async () => {
    const user = userEvent.setup();
    let resolveRename!: () => void;
    const onClose = vi.fn();
    const setActiveCueList = vi.fn();
    const renameCueList = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveRename = resolve;
        }),
    );

    renderWithAppProviders(<CueListManageModal onClose={onClose} />, {
      appState: cueListStateFixture,
      commands: { renameCueList, setActiveCueList },
    });

    await user.click(screen.getByRole("button", { name: "Rename Main" }));
    await user.click(screen.getByRole("button", { name: "Rename" }));

    const manageDialog = screen.getByRole("dialog", {
      name: "Manage Cue Lists",
    });
    expect(manageDialog).toHaveAttribute("inert");
    await user.click(
      within(manageDialog).getByRole("button", {
        name: "Close manage cue lists modal",
      }),
    );
    await user.click(
      within(manageDialog).getByRole("button", { name: "Verse" }),
    );
    await user.click(
      within(manageDialog).getByRole("button", { name: "Rename Verse" }),
    );

    expect(renameCueList).toHaveBeenCalledTimes(1);
    expect(renameCueList).toHaveBeenCalledWith("cue-list-main", "Main");
    expect(onClose).not.toHaveBeenCalled();
    expect(setActiveCueList).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Cue list name")).toHaveValue("Main");

    resolveRename();
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "Rename Cue List" }),
      ).not.toBeInTheDocument(),
    );
  });

  it("uses sibling semantic controls for selection and row actions", () => {
    renderWithAppProviders(<CueListManageModal onClose={vi.fn()} />, {
      appState: cueListStateFixture,
    });

    const select = screen.getByRole("button", { name: "Main" });
    expect(select).not.toContainElement(
      screen.getByRole("button", { name: "Rename Main" }),
    );
    expect(select).not.toContainElement(
      screen.getByRole("button", { name: "Delete Main" }),
    );
    expect(select.closest("button")).toBe(select);
  });

  it("keeps the name dialog open when creation rejects", async () => {
    const user = userEvent.setup();
    const createCueList = vi.fn().mockRejectedValue(new Error("rejected"));

    renderWithAppProviders(<CueListManageModal onClose={vi.fn()} />, {
      appState: cueListStateFixture,
      commands: { createCueList },
    });

    await user.click(screen.getByRole("button", { name: "New Cue List" }));
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(screen.getByRole("dialog", { name: "New Cue List" })).toBeVisible();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Create" })).toBeEnabled(),
    );
  });

  it.each([
    ["Rename Verse", "{Enter}"],
    ["Rename Verse", " "],
    ["Delete Verse", "{Enter}"],
    ["Delete Verse", " "],
    ["Drag Verse", "{Enter}"],
    ["Drag Verse", " "],
  ])(
    "does not select or close from nested %s control with %j",
    async (accessibleName, key) => {
      const user = userEvent.setup();
      const onClose = vi.fn();
      const setActiveCueList = vi.fn();

      renderWithAppProviders(<CueListManageModal onClose={onClose} />, {
        appState: cueListStateFixture,
        commands: { setActiveCueList },
      });

      const control = screen.getByLabelText(accessibleName);
      control.focus();
      await user.keyboard(key);

      expect(setActiveCueList).not.toHaveBeenCalled();
      expect(onClose).not.toHaveBeenCalled();
    },
  );

  it.each(["{Enter}", " "])(
    "selects a cue list with the keyboard using %j",
    async (key) => {
      const user = userEvent.setup();
      const onClose = vi.fn();
      const setActiveCueList = vi.fn();

      renderWithAppProviders(<CueListManageModal onClose={onClose} />, {
        appState: cueListStateFixture,
        commands: { setActiveCueList },
      });

      screen.getByRole("button", { name: "Verse" }).focus();
      await user.keyboard(key);

      expect(setActiveCueList).toHaveBeenCalledWith("cue-list-verse");
      expect(onClose).toHaveBeenCalledTimes(1);
    },
  );

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

import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { cueListStateFixture } from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import { CueListManageModal } from "./CueListManageModal";

describe("CueListManageModal", () => {
  it("shows cue lists and opens a delete confirmation", async () => {
    const user = userEvent.setup();
    renderWithAppProviders(<CueListManageModal onClose={vi.fn()} />, {
      appState: cueListStateFixture,
      commands: {
        deleteCueList: vi.fn(),
      },
    });

    expect(
      screen.getByRole("dialog", { name: /Manage Cue Lists/i }),
    ).toBeInTheDocument();
    expect(screen.getByText("Main")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Delete Main/i }));

    expect(
      screen.getByRole("dialog", { name: /Delete Cue List/i }),
    ).toBeInTheDocument();
  });
});

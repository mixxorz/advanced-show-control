import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderWithAppProviders } from "../test/render";
import { CueListNameModal } from "./CueListNameModal";

function renderNameModal(
  onSubmit: (name: string) => void | Promise<void>,
  onCancel = vi.fn(),
) {
  renderWithAppProviders(
    <CueListNameModal
      initialName="Initial"
      onCancel={onCancel}
      onSubmit={onSubmit}
      submitLabel="Create"
      title="New Cue List"
    />,
  );
  return { onCancel };
}

describe("CueListNameModal", () => {
  it("allows only one submission while the command is pending and ignores Escape", async () => {
    const user = userEvent.setup();
    let resolveSubmit!: () => void;
    const onSubmit = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveSubmit = resolve;
        }),
    );
    const { onCancel } = renderNameModal(onSubmit);
    const submit = screen.getByRole("button", { name: "Create" });

    await user.click(submit);

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(submit).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();

    await user.click(submit);
    await user.keyboard("{Escape}");

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onCancel).not.toHaveBeenCalled();

    resolveSubmit();
    await waitFor(() => expect(submit).toBeEnabled());
  });

  it("remains open and permits retry after submission rejects", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn().mockRejectedValueOnce(new Error("rejected"));
    renderNameModal(onSubmit);

    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(screen.getByRole("dialog", { name: "New Cue List" })).toBeVisible();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Create" })).toBeEnabled(),
    );
  });

  it("cancels with Escape without submitting when idle", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    const { onCancel } = renderNameModal(onSubmit);

    await user.keyboard("{Escape}");

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it.each(["", "  Bridge  "])(
    "submits the unchanged name payload %j",
    async (name) => {
      const user = userEvent.setup();
      const onSubmit = vi.fn();
      renderNameModal(onSubmit);
      fireEvent.change(screen.getByLabelText("Cue list name"), {
        target: { value: name },
      });

      await user.click(screen.getByRole("button", { name: "Create" }));

      expect(onSubmit).toHaveBeenCalledWith(name);
    },
  );
});

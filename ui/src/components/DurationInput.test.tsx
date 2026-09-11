import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderWithAppProviders } from "../test/render";
import { DurationInput } from "./DurationInput";

describe("DurationInput", () => {
  it.each([
    ["reports failure", vi.fn(async () => false)],
    ["rejects", vi.fn(async () => Promise.reject(new Error("failed")))],
  ])(
    "restores the projected duration when the command %s",
    async (_, command) => {
      const user = userEvent.setup();
      renderWithAppProviders(
        <DurationInput internalSceneId="scene-1" durationMs={2500} />,
        { commands: { setSceneDurationMs: command } },
      );

      const input = screen.getByRole("textbox", { name: "X-Fade" });
      await user.clear(input);
      await user.type(input, "3{Enter}");

      await waitFor(() => expect(input).toHaveValue("2.5s"));
      expect(command).toHaveBeenCalledTimes(1);
      expect(command).toHaveBeenCalledWith("scene-1", 3000);
    },
  );

  it("commits Enter only once when the resulting blur fires", async () => {
    const user = userEvent.setup();
    const setSceneDurationMs = vi.fn(async () => true);
    renderWithAppProviders(
      <DurationInput internalSceneId="scene-1" durationMs={2500} />,
      { commands: { setSceneDurationMs } },
    );

    const input = screen.getByRole("textbox", { name: "X-Fade" });
    await user.clear(input);
    await user.type(input, "3{Enter}");

    await waitFor(() => expect(input).toHaveValue("3.0s"));
    expect(setSceneDurationMs).toHaveBeenCalledTimes(1);
  });

  it("names both non-submit step buttons and uses the shared normalization", async () => {
    const user = userEvent.setup();
    const setSceneDurationMs = vi.fn(async () => true);
    renderWithAppProviders(
      <DurationInput internalSceneId="scene-1" durationMs={500} />,
      { commands: { setSceneDurationMs } },
    );

    const increase = screen.getByRole("button", { name: "Increase X-Fade" });
    const decrease = screen.getByRole("button", { name: "Decrease X-Fade" });
    expect(increase).toHaveAttribute("type", "button");
    expect(decrease).toHaveAttribute("type", "button");

    await user.click(increase);
    await user.click(decrease);

    expect(setSceneDurationMs).toHaveBeenNthCalledWith(1, "scene-1", 1500);
    expect(setSceneDurationMs).toHaveBeenNthCalledWith(2, "scene-1", 0);
  });
});

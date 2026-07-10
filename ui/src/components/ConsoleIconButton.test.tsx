import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ConsoleIconButton } from "./ConsoleIconButton";

describe("ConsoleIconButton", () => {
  it("forwards native button event handlers", () => {
    const onPointerDown = vi.fn();

    render(
      <ConsoleIconButton aria-label="Delete" onPointerDown={onPointerDown}>
        X
      </ConsoleIconButton>,
    );

    fireEvent.pointerDown(screen.getByRole("button", { name: "Delete" }));

    expect(onPointerDown).toHaveBeenCalledTimes(1);
  });
});

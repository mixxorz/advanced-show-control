import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  KeyboardProvider,
  useKeyboardHandler,
  useShortcutCapture,
} from "./keyboard";

describe("KeyboardProvider", () => {
  it("dispatches enabled handlers by priority and stops after handled", () => {
    const low = vi.fn(() => "handled" as const);
    const high = vi.fn(() => "handled" as const);

    function Harness() {
      useKeyboardHandler({
        id: "low",
        priority: 10,
        handleKeyDown: low,
      });
      useKeyboardHandler({
        id: "high",
        priority: 100,
        handleKeyDown: high,
      });
      return null;
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    fireKeyDown("k");

    expect(high).toHaveBeenCalledTimes(1);
    expect(low).not.toHaveBeenCalled();
  });

  it("continues dispatch when a higher-priority handler ignores the event", () => {
    const low = vi.fn(() => "handled" as const);
    const high = vi.fn(() => "ignored" as const);

    function Harness() {
      useKeyboardHandler({ id: "low", priority: 10, handleKeyDown: low });
      useKeyboardHandler({ id: "high", priority: 100, handleKeyDown: high });
      return null;
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    fireKeyDown("k");

    expect(high).toHaveBeenCalledTimes(1);
    expect(low).toHaveBeenCalledTimes(1);
  });

  it("captures a non-modifier key with modifiers and exits capture mode", () => {
    const onCapture = vi.fn();

    function Harness() {
      const capture = useShortcutCapture();
      return (
        <button
          type="button"
          onClick={() => capture.startCapture({ id: "go", onCapture })}
        >
          {capture.isCapturing("go") ? "capturing" : "idle"}
        </button>
      );
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    fireEvent.click(screen.getByRole("button"));
    expect(screen.getByText("capturing")).toBeInTheDocument();

    fireKeyDown("Enter", { shiftKey: true });

    expect(onCapture).toHaveBeenCalledWith({
      key: "Enter",
      modifiers: { shift: true, control: false, alt: false, meta: false },
    });
    expect(screen.getByText("idle")).toBeInTheDocument();
  });

  it("keeps capture active for modifier-only keys", () => {
    const onCapture = vi.fn();

    function Harness() {
      const capture = useShortcutCapture();
      return (
        <button
          type="button"
          onClick={() => capture.startCapture({ id: "go", onCapture })}
        >
          {capture.isCapturing("go") ? "capturing" : "idle"}
        </button>
      );
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    fireEvent.click(screen.getByRole("button"));
    fireKeyDown("Shift", { shiftKey: true });

    expect(onCapture).not.toHaveBeenCalled();
    expect(screen.getByText("capturing")).toBeInTheDocument();
  });

  it("cancels capture on Escape", () => {
    const onCapture = vi.fn();
    const onCancel = vi.fn();

    function Harness() {
      const capture = useShortcutCapture();
      return (
        <button
          type="button"
          onClick={() =>
            capture.startCapture({ id: "go", onCapture, onCancel })
          }
        >
          {capture.isCapturing("go") ? "capturing" : "idle"}
        </button>
      );
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    fireEvent.click(screen.getByRole("button"));
    fireKeyDown("Escape");

    expect(onCapture).not.toHaveBeenCalled();
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(screen.getByText("idle")).toBeInTheDocument();
  });

  it("captures Tab while capture mode is active", () => {
    const onCapture = vi.fn();

    function Harness() {
      const capture = useShortcutCapture();
      return (
        <button
          type="button"
          onClick={() => capture.startCapture({ id: "go", onCapture })}
        >
          capture
        </button>
      );
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    fireEvent.click(screen.getByRole("button"));
    fireKeyDown("Tab");

    expect(onCapture).toHaveBeenCalledWith({
      key: "Tab",
      modifiers: { shift: false, control: false, alt: false, meta: false },
    });
  });
});

function fireKeyDown(key: string, init: KeyboardEventInit = {}) {
  act(() => {
    window.dispatchEvent(
      new KeyboardEvent("keydown", {
        key,
        bubbles: true,
        cancelable: true,
        ...init,
      }),
    );
  });
}

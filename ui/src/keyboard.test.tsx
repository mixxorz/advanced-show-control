import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  isActionShortcutBlocked,
  KeyboardProvider,
  shortcutKeyFromEvent,
  shortcutMatchesEvent,
  useKeyboardHandler,
  useShortcutCapture,
} from "./keyboard";

describe("KeyboardProvider", () => {
  it.each([
    ["input", document.createElement("input"), true],
    ["textarea", document.createElement("textarea"), true],
    ["select", document.createElement("select"), true],
    ["content-editable descendant", createContentEditableDescendant(), true],
    ["button", document.createElement("button"), false],
    ["dialog descendant", createDialogDescendant(), true],
  ])(
    "classifies a %s target for action shortcuts",
    (_description, target, expected) => {
      let blocked = false;
      const listener = (originalEvent: KeyboardEvent) => {
        blocked = isActionShortcutBlocked({
          code: originalEvent.code,
          key: originalEvent.key,
          modifiers: {
            shift: originalEvent.shiftKey,
            control: originalEvent.ctrlKey,
            alt: originalEvent.altKey,
            meta: originalEvent.metaKey,
          },
          repeat: originalEvent.repeat,
          originalEvent,
        });
      };
      window.addEventListener("keydown", listener);
      const root = target.parentElement ?? target;
      document.body.append(root);

      fireEvent.keyDown(target, { key: "c", code: "KeyC" });

      root.remove();
      window.removeEventListener("keydown", listener);
      expect(blocked).toBe(expected);
    },
  );

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

  it("propagates OS key repeat state to handlers", () => {
    const repeats: boolean[] = [];

    function Harness() {
      useKeyboardHandler({
        id: "repeat-recorder",
        priority: 1,
        handleKeyDown: (event) => {
          repeats.push(event.repeat);
          return "handled";
        },
      });
      return null;
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    fireKeyDown("k", { repeat: true });

    expect(repeats).toEqual([true]);
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

  it("stores shifted number keys as the unshifted key plus Shift modifier", () => {
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
    fireKeyDown("@", { code: "Digit2", shiftKey: true });

    expect(onCapture).toHaveBeenCalledWith({
      key: "2",
      modifiers: { shift: true, control: false, alt: false, meta: false },
    });
  });

  it("stores unknown printable physical keys by code instead of shifted glyph", () => {
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
    fireKeyDown(">", { code: "IntlBackslash", shiftKey: true });

    expect(onCapture).toHaveBeenCalledWith({
      key: "IntlBackslash",
      modifiers: { shift: true, control: false, alt: false, meta: false },
    });
  });

  it("normalizes comparable shortcut keys from keydown events", () => {
    const seen: string[] = [];

    function Harness() {
      useKeyboardHandler({
        id: "recorder",
        priority: 1,
        handleKeyDown: (event) => {
          seen.push(shortcutKeyFromEvent(event));
          return "handled";
        },
      });
      return null;
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    fireKeyDown(" ", { code: "Space" });
    fireKeyDown("q", { code: "KeyQ" });
    fireKeyDown("@", { code: "Digit2", shiftKey: true });

    expect(seen).toEqual(["Space", "Q", "2"]);
  });

  it("matches shortcuts by comparable key and modifiers", () => {
    const matches: boolean[] = [];

    function Harness() {
      useKeyboardHandler({
        id: "matcher",
        priority: 1,
        handleKeyDown: (event) => {
          matches.push(
            shortcutMatchesEvent(
              {
                key: "S",
                modifiers: {
                  shift: true,
                  control: true,
                  alt: false,
                  meta: false,
                },
              },
              event,
            ),
          );
          return "handled";
        },
      });
      return null;
    }

    render(
      <KeyboardProvider>
        <Harness />
      </KeyboardProvider>,
    );

    fireKeyDown("S", { code: "KeyS", shiftKey: true, ctrlKey: true });
    fireKeyDown("s", { code: "KeyS", ctrlKey: true });

    expect(matches).toEqual([true, false]);
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

function createContentEditableDescendant() {
  const editor = document.createElement("div");
  editor.setAttribute("contenteditable", "true");
  const descendant = document.createElement("span");
  editor.append(descendant);
  return descendant;
}

function createDialogDescendant() {
  const dialog = document.createElement("div");
  dialog.setAttribute("role", "dialog");
  const descendant = document.createElement("span");
  dialog.append(descendant);
  return descendant;
}

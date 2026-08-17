import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  isActionShortcutBlocked,
  KeyboardProvider,
  shortcutKeyFromEvent,
  shortcutKeysEqual,
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

  it("blocks action shortcuts when an aria-modal dialog is open outside the event target", () => {
    const modal = document.createElement("div");
    modal.setAttribute("aria-modal", "true");
    modal.setAttribute("role", "dialog");
    const button = document.createElement("button");
    document.body.append(modal, button);
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

    fireEvent.keyDown(button, { key: "c", code: "KeyC" });

    window.removeEventListener("keydown", listener);
    modal.remove();
    button.remove();
    expect(blocked).toBe(true);
  });

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

  it("does not cancel another owner's capture when an owner unmounts", () => {
    const onSecondCapture = vi.fn();

    function CaptureButton(props: {
      ownerId: string;
      id: string;
      onCapture: (shortcut: { key: string }) => void;
    }) {
      const capture = useShortcutCapture(props.ownerId);
      return (
        <button
          type="button"
          onClick={() =>
            capture.startCapture({ id: props.id, onCapture: props.onCapture })
          }
        >
          {props.id}
        </button>
      );
    }

    function Harness(props: { showFirst: boolean }) {
      return (
        <>
          {props.showFirst ? (
            <CaptureButton
              ownerId="first-owner"
              id="first"
              onCapture={vi.fn()}
            />
          ) : null}
          <CaptureButton
            ownerId="second-owner"
            id="second"
            onCapture={onSecondCapture}
          />
        </>
      );
    }

    const { rerender } = render(
      <KeyboardProvider>
        <Harness showFirst />
      </KeyboardProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "first" }));
    fireEvent.click(screen.getByRole("button", { name: "second" }));
    rerender(
      <KeyboardProvider>
        <Harness showFirst={false} />
      </KeyboardProvider>,
    );

    fireKeyDown("Enter");

    expect(onSecondCapture).toHaveBeenCalledWith({
      key: "Enter",
      modifiers: { shift: false, control: false, alt: false, meta: false },
    });
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

  it("matches lowercase projected shortcut keys against keydown events", () => {
    const matches: boolean[] = [];

    function Harness() {
      useKeyboardHandler({
        id: "matcher",
        priority: 1,
        handleKeyDown: (event) => {
          matches.push(
            shortcutMatchesEvent(
              {
                key: "c",
                modifiers: {
                  shift: false,
                  control: false,
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

    fireKeyDown("c", { code: "KeyC" });

    expect(matches).toEqual([true]);
  });

  it("matches persisted Unicode shortcut labels against character keys", () => {
    expect(
      shortcutMatchesEvent(
        {
          key: "É",
          modifiers: { shift: false, control: false, alt: false, meta: false },
        },
        {
          key: "é",
          code: "KeyE",
          modifiers: { shift: false, control: false, alt: false, meta: false },
          repeat: false,
          originalEvent: new KeyboardEvent("keydown"),
        },
      ),
    ).toBe(true);
  });

  it("keeps physical code matching primary over character keys", () => {
    expect(
      shortcutMatchesEvent(
        {
          key: "Q",
          modifiers: { shift: false, control: false, alt: false, meta: false },
        },
        {
          key: "a",
          code: "KeyQ",
          modifiers: { shift: false, control: false, alt: false, meta: false },
          repeat: false,
          originalEvent: new KeyboardEvent("keydown"),
        },
      ),
    ).toBe(true);
  });

  it.each([
    ["i", "I"],
    ["é", "É"],
    ["ß", "SS"],
  ])("matches Unicode shortcut keys case-insensitively", (left, right) => {
    expect(shortcutKeysEqual(left, right)).toBe(true);
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

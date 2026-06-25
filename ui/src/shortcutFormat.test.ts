import { describe, expect, it } from "vitest";
import { formatShortcut } from "./shortcutFormat";
import type { KeyboardShortcut } from "./types";

describe("formatShortcut", () => {
  it("uses macOS symbols", () => {
    expect(formatShortcut(shortcut("c", { meta: true }), "mac")).toBe("⌘C");
    expect(formatShortcut(shortcut("2", { shift: true }), "mac")).toBe("⇧2");
    expect(
      formatShortcut(shortcut("Enter", { shift: true, alt: true }), "mac"),
    ).toBe("⇧⌥Enter");
  });

  it("uses non-macOS labels", () => {
    expect(formatShortcut(shortcut("c", { control: true }), "windows")).toBe(
      "Ctrl + C",
    );
    expect(formatShortcut(shortcut("2", { shift: true }), "windows")).toBe(
      "Shift + 2",
    );
    expect(formatShortcut(shortcut("Tab", { meta: true }), "windows")).toBe(
      "Win + Tab",
    );
    expect(formatShortcut(shortcut("Tab", { meta: true }), "linux")).toBe(
      "Meta + Tab",
    );
  });

  it("formats common key labels", () => {
    expect(formatShortcut(shortcut(" ", {}), "windows")).toBe("Space");
    expect(formatShortcut(shortcut("ArrowRight", {}), "windows")).toBe("Right");
  });
});

function shortcut(
  key: string,
  modifiers: Partial<KeyboardShortcut["modifiers"]>,
): KeyboardShortcut {
  return {
    key,
    modifiers: {
      shift: false,
      control: false,
      alt: false,
      meta: false,
      ...modifiers,
    },
  };
}

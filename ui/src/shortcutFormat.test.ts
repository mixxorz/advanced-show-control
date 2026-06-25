import { describe, expect, it } from "vitest";
import { formatShortcut } from "./shortcutFormat";
import type { KeyboardShortcut } from "./types";

describe("formatShortcut", () => {
  it("uses macOS symbols", () => {
    expect(formatShortcut(shortcut("c", { meta: true }), "mac")).toBe("⌘C");
    expect(
      formatShortcut(shortcut("Enter", { shift: true, alt: true }), "mac"),
    ).toBe("⇧⌥Enter");
  });

  it("uses non-macOS labels", () => {
    expect(formatShortcut(shortcut("c", { control: true }), "other")).toBe(
      "Ctrl + C",
    );
    expect(formatShortcut(shortcut("Tab", { meta: true }), "other")).toBe(
      "Win + Tab",
    );
  });

  it("formats common key labels", () => {
    expect(formatShortcut(shortcut(" ", {}), "other")).toBe("Space");
    expect(formatShortcut(shortcut("ArrowRight", {}), "other")).toBe("Right");
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

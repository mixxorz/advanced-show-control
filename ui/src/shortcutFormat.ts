import type { KeyboardShortcut } from "./types";

export type ShortcutPlatform = "mac" | "other";

export function detectShortcutPlatform(): ShortcutPlatform {
  const userAgentData = navigator as Navigator & {
    userAgentData?: { platform?: string };
  };
  const platform = userAgentData.userAgentData?.platform ?? navigator.platform;
  return /mac|iphone|ipad|ipod/i.test(platform) ? "mac" : "other";
}

export function formatShortcut(
  shortcut: KeyboardShortcut,
  platform: ShortcutPlatform = detectShortcutPlatform(),
) {
  const modifiers = shortcut.modifiers;
  const parts =
    platform === "mac"
      ? macModifierParts(modifiers)
      : otherModifierParts(modifiers);
  parts.push(formatKey(shortcut.key));
  return platform === "mac" ? parts.join("") : parts.join(" + ");
}

function macModifierParts(modifiers: KeyboardShortcut["modifiers"]) {
  const parts: string[] = [];
  if (modifiers.shift) parts.push("⇧");
  if (modifiers.control) parts.push("⌃");
  if (modifiers.alt) parts.push("⌥");
  if (modifiers.meta) parts.push("⌘");
  return parts;
}

function otherModifierParts(modifiers: KeyboardShortcut["modifiers"]) {
  const parts: string[] = [];
  if (modifiers.shift) parts.push("Shift");
  if (modifiers.control) parts.push("Ctrl");
  if (modifiers.alt) parts.push("Alt");
  if (modifiers.meta) parts.push("Win");
  return parts;
}

function formatKey(key: string) {
  if (key === " " || key === "Spacebar") return "Space";
  if (key.length === 1) return key.toUpperCase();
  if (key === "ArrowUp") return "Up";
  if (key === "ArrowDown") return "Down";
  if (key === "ArrowLeft") return "Left";
  if (key === "ArrowRight") return "Right";
  return key;
}

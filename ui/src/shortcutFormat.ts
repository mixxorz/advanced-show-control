import type { KeyboardShortcut } from "./types";

export type ShortcutPlatform = "mac" | "windows" | "linux";

export function detectShortcutPlatform(): ShortcutPlatform {
  const userAgentData = navigator as Navigator & {
    userAgentData?: { platform?: string };
  };
  const platform = userAgentData.userAgentData?.platform ?? navigator.platform;
  if (/mac|iphone|ipad|ipod/i.test(platform)) return "mac";
  if (/win/i.test(platform)) return "windows";
  return "linux";
}

export function formatShortcut(
  shortcut: KeyboardShortcut,
  platform: ShortcutPlatform = detectShortcutPlatform(),
) {
  const modifiers = shortcut.modifiers;
  const parts = modifierParts(modifiers, platform);
  parts.push(formatKey(shortcut.key));
  return platform === "mac" ? parts.join("") : parts.join(" + ");
}

function modifierParts(
  modifiers: KeyboardShortcut["modifiers"],
  platform: ShortcutPlatform,
) {
  return platform === "mac"
    ? macModifierParts(modifiers)
    : textModifierParts(modifiers, platform);
}

function macModifierParts(modifiers: KeyboardShortcut["modifiers"]) {
  const parts: string[] = [];
  if (modifiers.shift) parts.push("⇧");
  if (modifiers.control) parts.push("⌃");
  if (modifiers.alt) parts.push("⌥");
  if (modifiers.meta) parts.push("⌘");
  return parts;
}

function textModifierParts(
  modifiers: KeyboardShortcut["modifiers"],
  platform: Exclude<ShortcutPlatform, "mac">,
) {
  const parts: string[] = [];
  if (modifiers.shift) parts.push("Shift");
  if (modifiers.control) parts.push("Ctrl");
  if (modifiers.alt) parts.push("Alt");
  if (modifiers.meta) parts.push(platform === "windows" ? "Win" : "Meta");
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

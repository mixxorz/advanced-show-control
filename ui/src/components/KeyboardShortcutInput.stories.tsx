import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ShortcutPlatform } from "../shortcutFormat";
import type { KeyboardShortcut } from "../types";
import { KeyboardShortcutInput } from "./KeyboardShortcutInput";

const meta = {
  title: "Primitives/KeyboardShortcutInput",
  component: KeyboardShortcutInput,
  parameters: {
    layout: "centered",
  },
} satisfies Meta<typeof KeyboardShortcutInput>;

export default meta;
type Story = StoryObj<typeof meta>;

const shortcuts: { label: string; shortcut: KeyboardShortcut }[] = [
  { label: "GO", shortcut: shortcut("Space", {}) },
  { label: "Cue", shortcut: shortcut("C", { control: true }) },
  { label: "Shift digit", shortcut: shortcut("2", { shift: true }) },
  { label: "Combo", shortcut: shortcut("K", { shift: true, meta: true }) },
  { label: "Arrow", shortcut: shortcut("ArrowRight", { alt: true }) },
  {
    label: "Long combo",
    shortcut: shortcut("ArrowRight", {
      shift: true,
      control: true,
      alt: true,
      meta: true,
    }),
  },
];

const platforms: { label: string; platform: ShortcutPlatform }[] = [
  { label: "macOS", platform: "mac" },
  { label: "Windows", platform: "windows" },
  { label: "Linux", platform: "linux" },
];

export const Variants: Story = {
  args: {
    isCapturing: false,
    label: "Shortcut primitive",
    onStartCapture: () => {},
    shortcut: shortcut("K", { meta: true }),
  },
  render: () => (
    <div className="grid gap-6 rounded-console-panel border border-console-line bg-console-chrome p-5">
      <div className="grid grid-cols-[7rem_repeat(3,12rem)] gap-3 text-sm">
        <span />
        {platforms.map((platform) => (
          <span
            className="text-xs uppercase tracking-[0.08em] text-console-primary"
            key={platform.platform}
          >
            {platform.label}
          </span>
        ))}
        {shortcuts.map((entry) => (
          <ShortcutRow entry={entry} key={entry.label} />
        ))}
      </div>

      <div className="grid grid-cols-[7rem_12rem] items-center gap-3">
        <span className="text-sm text-console-muted">Active</span>
        <KeyboardShortcutInput
          isCapturing
          label="Active shortcut capture"
          onStartCapture={() => {}}
          platform="mac"
          shortcut={shortcut("K", { meta: true })}
        />
      </div>
    </div>
  ),
};

function ShortcutRow(props: {
  entry: { label: string; shortcut: KeyboardShortcut };
}) {
  return (
    <>
      <span className="self-center text-sm text-console-muted">
        {props.entry.label}
      </span>
      {platforms.map((platform) => (
        <KeyboardShortcutInput
          isCapturing={false}
          key={platform.platform}
          label={`${props.entry.label} ${platform.label}`}
          onStartCapture={() => {}}
          platform={platform.platform}
          shortcut={props.entry.shortcut}
        />
      ))}
    </>
  );
}

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

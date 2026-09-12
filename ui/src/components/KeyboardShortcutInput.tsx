import { useId } from "react";
import { formatShortcut, type ShortcutPlatform } from "../shortcutFormat";
import type { KeyboardShortcut } from "../types";

const settingControlText = "font-mono text-sm uppercase";
const settingControlSize = "h-9 w-48";

/**
 * @cc [owner:mixxorz,label:product;accessibility] shortcut-capture-presentation
 * The controlled input MUST invoke `onStartCapture` when activated and display `...` instead of the
 * assigned shortcut while capturing. Any conflict MUST leave the shortcut unchanged, be announced
 * as an alert, and be associated with the capture button through `aria-describedby`.
 */
export function KeyboardShortcutInput(props: {
  label: string;
  shortcut: KeyboardShortcut;
  isCapturing: boolean;
  onStartCapture: () => void;
  platform?: ShortcutPlatform;
  conflictMessage?: string;
}) {
  const conflictId = useId();
  const displayValue = props.isCapturing
    ? "..."
    : formatShortcut(props.shortcut, props.platform);

  return (
    <div className="flex items-center gap-3">
      <button
        aria-describedby={props.conflictMessage ? conflictId : undefined}
        aria-label={`Change ${props.label}`}
        className={`${settingControlSize} ${settingControlText} truncate rounded-console-control border px-3 py-1.5 text-center outline-none transition-colors hover:border-console-line-strong hover:text-accent-orange-hover active:border-accent-orange active:bg-accent-orange-active active:text-white focus:border-console-line-strong ${
          props.isCapturing
            ? "!border-status-warning bg-accent-orange-soft text-status-warning shadow-[0_0_0_1px_rgba(240,180,41,0.12)]"
            : "border-console-line bg-console-panel text-accent-orange"
        }`}
        title={displayValue}
        type="button"
        onClick={props.onStartCapture}
      >
        {displayValue}
      </button>
      {props.conflictMessage ? (
        <span
          aria-live="assertive"
          className="text-sm text-status-danger"
          id={conflictId}
          role="alert"
        >
          {props.conflictMessage}
        </span>
      ) : null}
    </div>
  );
}

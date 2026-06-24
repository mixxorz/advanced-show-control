import type { ReactNode } from "react";

type ConsoleButtonVariant =
  | "primary"
  | "secondary"
  | "ghost-primary"
  | "ghost-danger"
  | "danger"
  | "ghost-secondary";
type ConsoleButtonSize = "default" | "small" | "big";

export function ConsoleButton(props: {
  active?: boolean;
  children: ReactNode;
  disabled?: boolean;
  fullWidth?: boolean;
  onClick?: () => void;
  size?: ConsoleButtonSize;
  variant?: ConsoleButtonVariant;
}) {
  const variant = props.variant ?? "secondary";
  const size = props.size ?? "default";
  const sizeClass = {
    small: "min-w-16 px-3 py-1 text-sm",
    default: "min-h-11 px-5 py-2 text-[1.1rem]",
    big: "min-h-14 min-w-24 px-7 py-3 text-2xl",
  }[size];
  const widthClass = props.fullWidth ? "w-full" : "";
  const baseClass =
    "rounded-console-control border font-normal uppercase disabled:text-console-disabled";
  const className = props.active
    ? "border-accent-orange bg-accent-orange-active text-white hover:bg-accent-orange disabled:border-console-line disabled:bg-console-control"
    : {
        primary:
          "border-accent-orange bg-accent-orange-active text-white hover:bg-accent-orange disabled:border-console-line disabled:bg-console-control",
        danger:
          "border-status-danger bg-status-danger-active text-white hover:bg-status-danger disabled:border-console-line disabled:bg-console-control",
        secondary:
          "border-console-line bg-console-control text-console-primary hover:border-console-line-strong hover:bg-console-control-hover",
        "ghost-primary":
          "border-accent-orange bg-console-panel text-accent-orange hover:bg-accent-orange-soft hover:text-accent-orange-hover disabled:border-console-line disabled:bg-console-panel",
        "ghost-danger":
          "border-status-danger bg-console-panel text-status-danger hover:bg-console-control hover:text-status-danger disabled:border-console-line disabled:bg-console-panel",
        "ghost-secondary":
          "border-console-primary bg-console-panel text-console-primary hover:border-white hover:bg-console-control hover:text-white disabled:border-console-line disabled:bg-console-panel",
      }[variant];

  return (
    <button
      className={`${baseClass} ${sizeClass} ${widthClass} ${className}`}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}

import type { ButtonHTMLAttributes, ReactNode } from "react";

type ConsoleButtonVariant =
  | "primary"
  | "secondary"
  | "ghost-primary"
  | "ghost-danger"
  | "danger"
  | "ghost-secondary";
type ConsoleButtonSize = "default" | "small" | "big";

export function ConsoleButton(
  props: {
    active?: boolean;
    children: ReactNode;
    disabled?: boolean;
    fullWidth?: boolean;
    onClick?: ButtonHTMLAttributes<HTMLButtonElement>["onClick"];
    size?: ConsoleButtonSize;
    variant?: ConsoleButtonVariant;
  } & ButtonHTMLAttributes<HTMLButtonElement>,
) {
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
    ? "border-accent-orange bg-accent-orange-active text-white hover:bg-accent-orange disabled:border-console-line disabled:bg-console-control disabled:hover:bg-console-control disabled:hover:text-console-disabled"
    : {
        primary:
          "border-accent-orange bg-accent-orange-active text-white hover:bg-accent-orange disabled:border-console-line disabled:bg-console-control disabled:hover:bg-console-control disabled:hover:text-console-disabled",
        danger:
          "border-status-danger bg-status-danger-active text-white hover:bg-status-danger disabled:border-console-line disabled:bg-console-control disabled:hover:bg-console-control disabled:hover:text-console-disabled",
        secondary:
          "border-console-line bg-console-control text-console-primary hover:border-console-line-strong hover:bg-console-control-hover disabled:hover:border-console-line disabled:hover:bg-console-control disabled:hover:text-console-disabled",
        "ghost-primary":
          "border-accent-orange bg-console-panel text-accent-orange hover:bg-accent-orange-soft hover:text-accent-orange-hover disabled:border-console-line disabled:bg-console-panel disabled:hover:bg-console-panel disabled:hover:text-console-disabled",
        "ghost-danger":
          "border-status-danger bg-console-panel text-status-danger hover:bg-console-control hover:text-status-danger disabled:border-console-line disabled:bg-console-panel disabled:hover:bg-console-panel disabled:hover:text-console-disabled",
        "ghost-secondary":
          "border-console-primary bg-console-panel text-console-primary hover:border-white hover:bg-console-control hover:text-white disabled:border-console-line disabled:bg-console-panel disabled:hover:border-console-line disabled:hover:bg-console-panel disabled:hover:text-console-disabled",
      }[variant];

  return (
    <button
      className={`${baseClass} ${sizeClass} ${widthClass} ${className}`}
      aria-label={props["aria-label"]}
      disabled={props.disabled}
      onClick={props.onClick}
      type={props.type ?? "button"}
    >
      {props.children}
    </button>
  );
}

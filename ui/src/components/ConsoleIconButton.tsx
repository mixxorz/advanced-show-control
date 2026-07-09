import type { ButtonHTMLAttributes, ReactNode } from "react";

type ConsoleIconButtonVariant = "secondary" | "ghost-danger";
type ConsoleIconButtonSize = "small" | "default";

export function ConsoleIconButton(
  props: {
    "aria-label": string;
    children: ReactNode;
    disabled?: boolean;
    onClick?: ButtonHTMLAttributes<HTMLButtonElement>["onClick"];
    size?: ConsoleIconButtonSize;
    variant?: ConsoleIconButtonVariant;
  } & ButtonHTMLAttributes<HTMLButtonElement>,
) {
  const variant = props.variant ?? "secondary";
  const size = props.size ?? "default";
  const sizeClass = {
    small: "h-8 w-8",
    default: "h-11 w-11",
  }[size];
  const variantClass = {
    secondary:
      "text-console-secondary hover:bg-console-control-hover hover:text-console-primary focus-visible:bg-console-control-hover focus-visible:text-console-primary disabled:hover:bg-transparent disabled:hover:text-console-disabled",
    "ghost-danger":
      "text-status-danger hover:bg-console-control-hover hover:text-status-danger-hover focus-visible:bg-console-control-hover focus-visible:text-status-danger-hover disabled:hover:bg-transparent disabled:hover:text-console-disabled",
  }[variant];

  return (
    <button
      aria-label={props["aria-label"]}
      className={`inline-grid place-items-center rounded-console-control bg-transparent disabled:text-console-disabled ${sizeClass} ${variantClass}`}
      disabled={props.disabled}
      onClick={props.onClick}
      type={props.type ?? "button"}
    >
      {props.children}
    </button>
  );
}

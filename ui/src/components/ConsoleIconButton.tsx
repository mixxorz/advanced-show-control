import type { ButtonHTMLAttributes, ReactNode } from "react";

type ConsoleIconButtonVariant = "secondary" | "ghost-danger";
type ConsoleIconButtonSize = "small" | "default";

type ConsoleIconButtonProps = {
  "aria-label": string;
  children: ReactNode;
  size?: ConsoleIconButtonSize;
  variant?: ConsoleIconButtonVariant;
} & ButtonHTMLAttributes<HTMLButtonElement>;

/**
 * @cc [owner:mixxorz,label:accessibility] icon-button-native-semantics
 * The control MUST require an accessible label, preserve native button attributes including
 * `disabled`, and default to `type="button"` unless the caller explicitly supplies another type.
 */
export function ConsoleIconButton({
  size = "default",
  variant = "secondary",
  children,
  type = "button",
  className,
  ...buttonProps
}: ConsoleIconButtonProps) {
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
      {...buttonProps}
      className={`inline-grid place-items-center rounded-console-control bg-transparent disabled:text-console-disabled ${sizeClass} ${variantClass}${className ? ` ${className}` : ""}`}
      type={type}
    >
      {children}
    </button>
  );
}

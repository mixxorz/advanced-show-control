import type { ReactNode } from "react";

type ConsoleButtonVariant = "primary" | "secondary";

export function ConsoleButton(props: {
  active?: boolean;
  children: ReactNode;
  disabled?: boolean;
  onClick?: () => void;
  variant?: ConsoleButtonVariant;
}) {
  const variant = props.variant ?? "secondary";
  const className =
    props.active || variant === "primary"
      ? "rounded-console-control border border-accent-orange bg-accent-orange-active px-4 py-2 font-bold text-white hover:bg-accent-orange disabled:border-console-line disabled:bg-console-control disabled:text-console-disabled"
      : "rounded-console-control border border-console-line bg-console-control px-4 py-2 font-bold text-console-primary hover:border-console-line-strong hover:bg-console-control-hover disabled:text-console-disabled";

  return (
    <button
      className={className}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}

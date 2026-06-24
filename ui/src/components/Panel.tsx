import type { ReactNode } from "react";

type PanelVariant = "default" | "warning";

export function Panel(props: {
  children: ReactNode;
  className?: string;
  variant?: PanelVariant;
}) {
  const variantClass = {
    default: "border-console-line bg-console-panel",
    warning:
      "border-status-warning bg-accent-orange-soft shadow-[0_0_0_1px_rgba(240,180,41,0.12)]",
  }[props.variant ?? "default"];

  return (
    <section
      className={`rounded-console-panel border ${variantClass} ${props.className ?? ""}`}
    >
      {props.children}
    </section>
  );
}

import type { ReactNode } from "react";

export function StatusCell(props: {
  label: string;
  tone?: "default" | "current" | "cued" | "warning" | "danger";
  value: ReactNode;
}) {
  const tone = props.tone ?? "default";
  const valueClass = {
    default: "text-console-primary",
    current: "text-status-current",
    cued: "text-status-cued",
    warning: "text-status-warning",
    danger: "text-status-danger",
  }[tone];

  return (
    <div className="min-w-0 border-r border-console-line px-6 py-3 last:border-r-0">
      <div className="text-xs uppercase tracking-[0.08em] text-console-secondary">
        {props.label}
      </div>
      <div
        className={`mt-1 truncate font-mono text-lg font-medium ${valueClass}`}
      >
        {props.value}
      </div>
    </div>
  );
}

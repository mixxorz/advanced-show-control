import type { ReactNode } from "react";

export function StatusCell(props: {
  className?: string;
  font?: "ui" | "mono";
  label: string;
  tone?: "default" | "current" | "cued" | "warning" | "danger";
  value: ReactNode;
}) {
  const tone = props.tone ?? "default";
  const fontClass = props.font === "mono" ? "font-mono" : "font-ui";
  const valueClass = {
    default: "text-console-primary",
    current: "text-status-current",
    cued: "text-status-cued",
    warning: "text-status-warning",
    danger: "text-status-danger",
  }[tone];

  return (
    <div className="grid min-w-0 content-center border-r border-console-line px-6 py-3 last:border-r-0">
      <div className="text-xs uppercase tracking-[0.08em] text-console-secondary">
        {props.label}
      </div>
      <div
        className={`mt-1 truncate ${fontClass} text-lg font-normal ${valueClass} ${props.className ?? ""}`}
      >
        {props.value}
      </div>
    </div>
  );
}

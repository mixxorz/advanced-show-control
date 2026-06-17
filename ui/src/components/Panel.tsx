import type { ReactNode } from "react";

export function Panel(props: { children: ReactNode; className?: string }) {
  return (
    <section
      className={`rounded-console-panel border border-console-line bg-console-panel ${props.className ?? ""}`}
    >
      {props.children}
    </section>
  );
}

import type { ReactNode } from "react";

export function TopTab(props: {
  active: boolean;
  children: ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      className={
        props.active
          ? "border-x border-t border-console-line border-b-4 border-b-accent-orange bg-console-panel px-8 py-4 text-sm font-semibold uppercase tracking-[0.12em] text-accent-orange"
          : "border-x border-t border-console-line border-b border-b-console-line bg-console-chrome px-8 py-4 text-sm font-semibold uppercase tracking-[0.12em] text-console-secondary hover:text-console-primary"
      }
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}

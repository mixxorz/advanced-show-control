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
          ? "border-r border-console-line border-b-4 border-b-accent-orange bg-console-panel px-8 py-4 text-lg font-normal uppercase text-accent-orange"
          : "border-r border-console-line border-b-4 border-b-transparent bg-console-chrome px-8 py-4 text-lg font-normal uppercase text-console-secondary hover:text-console-primary"
      }
      onClick={props.onClick}
    >
      <span className="block translate-y-0.5">{props.children}</span>
    </button>
  );
}

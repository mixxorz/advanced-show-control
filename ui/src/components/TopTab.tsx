import type { ReactNode } from "react";

/**
 * @cc [owner:mixxorz,label:product] top-tab-activation
 * User activation MUST invoke `onClick` regardless of active state; `active` MUST NOT suppress or
 * synthesize selection callbacks.
 */
/**
 * @cc [owner:mixxorz,label:accessibility] top-tab-current-page
 * The active navigation button MUST expose `aria-current="page"`; inactive buttons MUST omit
 * `aria-current` so exactly the caller-selected tab is announced as current.
 */
export function TopTab(props: {
  active: boolean;
  children: ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      aria-current={props.active ? "page" : undefined}
      className={
        props.active
          ? "border-r border-console-line border-b-4 border-b-accent-orange bg-console-panel px-8 py-4 text-lg font-normal uppercase text-accent-orange"
          : "border-r border-console-line border-b-4 border-b-transparent bg-console-chrome px-8 py-4 text-lg font-normal uppercase text-console-secondary hover:text-console-primary"
      }
      onClick={props.onClick}
      type="button"
    >
      <span className="block translate-y-0.5">{props.children}</span>
    </button>
  );
}

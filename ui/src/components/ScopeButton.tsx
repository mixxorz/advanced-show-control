/**
 * @cc [owner:mixxorz,label:product;accessibility] scope-button-semantics
 * The control MUST remain a non-submit native button named by visible `label`, expose the
 * caller-provided summary through its `title`, mirror `active` through visual styling and
 * `aria-pressed` without gating interaction, and invoke `onClick` once per activation.
 */
export function ScopeButton(props: {
  active: boolean;
  label: string;
  onClick: () => void;
  title: string;
}) {
  return (
    <button
      aria-pressed={props.active}
      className={
        props.active
          ? "w-10 rounded-console-control border border-accent-orange bg-accent-orange-active px-2.5 py-1.5 font-mono text-sm font-normal text-white"
          : "w-10 rounded-console-control border border-console-line bg-console-control px-2.5 py-1.5 font-mono text-sm font-normal text-console-primary hover:bg-console-control-hover"
      }
      onClick={props.onClick}
      title={props.title}
      type="button"
    >
      {props.label}
    </button>
  );
}

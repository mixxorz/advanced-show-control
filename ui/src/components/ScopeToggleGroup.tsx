/**
 * @cc [owner:mixxorz,label:product;accessibility] scope-toggle-prop-mapping
 * FADER and PAN MUST remain separate non-submit native buttons. Each button's visual and
 * `aria-pressed` state and callback MUST map only to its corresponding enabled prop and toggle
 * callback; `size` MUST be applied consistently to both.
 */
export function ScopeToggleGroup(props: {
  fadersEnabled: boolean;
  panEnabled: boolean;
  onToggleFaders: () => void;
  onTogglePan: () => void;
  size?: "default" | "small";
}) {
  return (
    <div className="flex gap-2">
      <ScopeToggleButton
        active={props.fadersEnabled}
        label="FADER"
        onClick={props.onToggleFaders}
        size={props.size}
      />
      <ScopeToggleButton
        active={props.panEnabled}
        label="PAN"
        onClick={props.onTogglePan}
        size={props.size}
      />
    </div>
  );
}

function ScopeToggleButton(props: {
  active: boolean;
  label: string;
  onClick: () => void;
  size?: "default" | "small";
}) {
  const sizeClass =
    props.size === "small"
      ? "min-w-16 px-3 py-1 text-sm"
      : "min-h-11 px-5 py-2 text-[1.1rem]";
  const stateClass = props.active
    ? "border-accent-orange bg-accent-orange-active text-white hover:bg-accent-orange"
    : "border-console-line bg-console-control text-console-primary hover:border-console-line-strong hover:bg-console-control-hover";
  const contentClass = props.size === "small" ? "translate-y-px" : "";

  return (
    <button
      aria-pressed={props.active}
      className={`rounded-console-control border font-normal uppercase ${sizeClass} ${stateClass}`}
      onClick={props.onClick}
      type="button"
    >
      <span className={`inline-block ${contentClass}`}>{props.label}</span>
    </button>
  );
}

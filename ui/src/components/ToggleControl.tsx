const settingControlText = "font-mono text-sm uppercase";

export function ToggleControl(props: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      aria-label={props.label}
      aria-pressed={props.checked}
      className="relative h-9 w-32 rounded-console-control border border-console-line bg-console-panel p-1 outline-none transition-colors hover:border-console-line-strong focus:border-console-line-strong"
      type="button"
      onClick={() => props.onChange(!props.checked)}
    >
      <span
        className={`grid h-full w-1/2 place-items-center rounded-[calc(var(--radius-console-control)-0.25rem)] border bg-console-panel ${settingControlText} transition-all ${
          props.checked
            ? "translate-x-full border-accent-orange text-accent-orange"
            : "translate-x-0 border-console-line text-console-muted"
        }`}
      >
        {props.checked ? "ON" : "OFF"}
      </span>
    </button>
  );
}

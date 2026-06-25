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
      className="relative h-9 w-32 rounded-console-control border border-console-line bg-console-panel outline-none transition-colors hover:border-console-line-strong focus:border-console-line-strong"
      type="button"
      onClick={() => props.onChange(!props.checked)}
    >
      <span
        className={`absolute -left-px -top-px bottom-[-1px] grid w-[calc(50%+1px)] place-items-center rounded-console-control border bg-console-panel ${settingControlText} transition-transform ${
          props.checked
            ? "translate-x-[calc(100%-1px)] border-accent-orange text-accent-orange"
            : "translate-x-0 border-console-line text-console-muted"
        }`}
      >
        {props.checked ? "ON" : "OFF"}
      </span>
    </button>
  );
}

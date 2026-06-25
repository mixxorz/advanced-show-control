const settingControlText = "font-mono text-sm uppercase";

export function SelectControl(props: {
  label: string;
  value: string;
  options: { label: string; value: string }[];
  onChange: (value: string) => void;
}) {
  return (
    <div className="relative h-9 w-32">
      <select
        aria-label={props.label}
        className={`${settingControlText} h-full w-full appearance-none rounded-console-control border border-console-line bg-console-panel px-3 pr-7 text-accent-orange outline-none transition-colors hover:border-console-line-strong focus:border-console-line-strong`}
        value={props.value}
        onChange={(event) => props.onChange(event.target.value)}
      >
        {props.options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      <span className="pointer-events-none absolute right-3 top-1/2 h-0 w-0 -translate-y-1/2 border-x-[5px] border-t-[6px] border-x-transparent border-t-console-secondary" />
    </div>
  );
}

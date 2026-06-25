const settingControlText = "font-mono text-sm uppercase";

export function StepperControl(props: {
  label: string;
  min: number;
  max: number;
  value: number;
  onChange: (value: number) => void;
}) {
  function step(direction: 1 | -1) {
    props.onChange(
      Math.min(props.max, Math.max(props.min, props.value + direction)),
    );
  }

  return (
    <div className="group flex h-9 w-32 gap-1">
      <input
        aria-label={props.label}
        className={`${settingControlText} w-32 rounded-console-control border border-console-line bg-console-panel px-3 py-1.5 text-center text-accent-orange outline-none transition-colors group-hover:border-console-line-strong focus:border-console-line-strong`}
        readOnly
        type="text"
        value={props.value}
      />
      <div className="flex w-[2rem] shrink-0 flex-col gap-1">
        <button
          aria-label={`Increase ${props.label}`}
          className={`${settingControlText} grid flex-1 place-items-center rounded-console-control border border-console-line bg-console-panel leading-none text-accent-orange hover:border-console-line-strong hover:text-accent-orange-hover active:border-accent-orange active:bg-accent-orange-active active:text-white`}
          type="button"
          onClick={() => step(1)}
        >
          <StepperArrow direction="up" />
        </button>
        <button
          aria-label={`Decrease ${props.label}`}
          className={`${settingControlText} grid flex-1 place-items-center rounded-console-control border border-console-line bg-console-panel leading-none text-accent-orange hover:border-console-line-strong hover:text-accent-orange-hover active:border-accent-orange active:bg-accent-orange-active active:text-white`}
          type="button"
          onClick={() => step(-1)}
        >
          <StepperArrow direction="down" />
        </button>
      </div>
    </div>
  );
}

function StepperArrow(props: { direction: "up" | "down" }) {
  return (
    <svg
      aria-hidden="true"
      className="h-2.5 w-2.5 stroke-white"
      fill="none"
      viewBox="0 0 12 12"
    >
      <path
        d={props.direction === "up" ? "M3 7.5 6 4.5l3 3" : "M3 4.5 6 7.5l3-3"}
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
      />
    </svg>
  );
}

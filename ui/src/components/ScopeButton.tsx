export function ScopeButton(props: {
  active: boolean;
  label: string;
  onClick: () => void;
  title: string;
}) {
  return (
    <button
      className={
        props.active
          ? "min-w-10 rounded-console-control border border-accent-orange bg-accent-orange-active px-3 py-2 font-mono font-bold text-white"
          : "min-w-10 rounded-console-control border border-console-line bg-console-control px-3 py-2 font-mono font-bold text-console-primary hover:bg-console-control-hover"
      }
      onClick={props.onClick}
      title={props.title}
    >
      {props.label}
    </button>
  );
}

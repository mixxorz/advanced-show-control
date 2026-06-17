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
          ? "w-10 rounded-console-control border border-accent-orange bg-accent-orange-active px-2.5 py-1.5 font-mono text-sm font-normal text-white"
          : "w-10 rounded-console-control border border-console-line bg-console-control px-2.5 py-1.5 font-mono text-sm font-normal text-console-primary hover:bg-console-control-hover"
      }
      onClick={props.onClick}
      title={props.title}
    >
      {props.label}
    </button>
  );
}

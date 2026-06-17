export function StatusBadge(props: {
  label: string;
  tone: "neutral" | "warning" | "good";
}) {
  const tone =
    props.tone === "warning"
      ? "border-amber-500/60 bg-amber-950 text-amber-100"
      : props.tone === "good"
        ? "border-emerald-500/60 bg-emerald-950 text-emerald-100"
        : "border-slate-700 bg-slate-800 text-slate-200";

  return (
    <span className={`rounded-full border px-3 py-1 text-sm ${tone}`}>
      {props.label}
    </span>
  );
}

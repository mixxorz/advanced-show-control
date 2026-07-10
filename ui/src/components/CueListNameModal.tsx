import { useEffect, useState } from "react";
import { ConsoleButton } from "./ConsoleButton";

export function CueListNameModal(props: {
  title: string;
  initialName?: string;
  submitLabel: string;
  onSubmit: (name: string) => void | Promise<void>;
  onCancel: () => void;
}) {
  const [name, setName] = useState(props.initialName ?? "");
  const { onCancel } = props;

  function submitName() {
    void props.onSubmit(name);
  }

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onCancel();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onCancel]);

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-6">
      <section
        aria-label={props.title}
        aria-modal="true"
        className="max-w-md rounded-console-panel border border-console-line bg-console-panel p-6 shadow-2xl"
        role="dialog"
      >
        <h2 className="text-lg font-normal uppercase text-console-primary">
          {props.title}
        </h2>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            submitName();
          }}
        >
          <label className="mt-4 block text-sm uppercase tracking-[0.08em] text-console-secondary">
            Cue list name
            <input
              aria-label="Cue list name"
              autoFocus
              className="mt-2 w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 text-base text-console-primary outline-none focus:border-console-line-strong"
              onChange={(event) => setName(event.target.value)}
              value={name}
            />
          </label>
          <div className="mt-6 flex justify-end gap-3">
            <ConsoleButton
              onClick={props.onCancel}
              size="small"
              type="button"
              variant="secondary"
            >
              Cancel
            </ConsoleButton>
            <ConsoleButton size="small" type="submit" variant="primary">
              {props.submitLabel}
            </ConsoleButton>
          </div>
        </form>
      </section>
    </div>
  );
}

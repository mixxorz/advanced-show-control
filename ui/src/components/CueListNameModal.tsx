import { useEffect, useRef, useState } from "react";
import { ConsoleButton } from "./ConsoleButton";

/**
 * @cc [owner:mixxorz,label:product] cue-list-name-validation-boundary
 * Submission MUST pass the current input string unchanged, including whitespace or an empty value,
 * to `onSubmit`; this modal MUST NOT trim, reject, or otherwise perform authoritative name
 * validation.
 */
/**
 * @cc [owner:mixxorz,label:product;accessibility] cue-list-name-single-flight
 * At most one `onSubmit` call MAY be pending. While it is pending, repeat submission and cancellation
 * by button or Escape MUST be disabled. Rejection MUST keep the modal open and restore its actions;
 * when idle, Cancel or Escape MUST invoke `onCancel` without submitting. Successful dismissal MUST
 * remain the parent's responsibility after its `onSubmit` work succeeds.
 */
export function CueListNameModal(props: {
  title: string;
  initialName?: string;
  submitLabel: string;
  onSubmit: (name: string) => void | Promise<void>;
  onCancel: () => void;
}) {
  const [name, setName] = useState(props.initialName ?? "");
  const [submitting, setSubmitting] = useState(false);
  const submittingRef = useRef(false);
  const { onCancel } = props;

  async function submitName() {
    if (submittingRef.current) return;

    submittingRef.current = true;
    setSubmitting(true);
    try {
      await props.onSubmit(name);
    } catch {
      // Keep the modal available for retry.
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  }

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !submittingRef.current) {
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
              disabled={submitting}
              onClick={props.onCancel}
              size="small"
              type="button"
              variant="secondary"
            >
              Cancel
            </ConsoleButton>
            <ConsoleButton
              disabled={submitting}
              size="small"
              type="submit"
              variant="primary"
            >
              {props.submitLabel}
            </ConsoleButton>
          </div>
        </form>
      </section>
    </div>
  );
}

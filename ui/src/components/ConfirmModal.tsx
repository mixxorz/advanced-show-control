import { useEffect } from "react";
import { ConsoleButton } from "./ConsoleButton";

export function ConfirmModal(props: {
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { onCancel } = props;

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
        aria-modal="true"
        aria-label={props.title}
        className="max-w-md rounded-console-panel border border-console-line bg-console-panel p-6 shadow-2xl"
        role="dialog"
      >
        <h2 className="text-lg font-normal uppercase text-console-primary">
          {props.title}
        </h2>
        <p className="mt-3 text-sm text-console-secondary">{props.message}</p>
        <div className="mt-6 flex justify-end gap-3">
          <ConsoleButton
            onClick={props.onCancel}
            size="small"
            variant="secondary"
          >
            {props.cancelLabel}
          </ConsoleButton>
          <ConsoleButton
            onClick={props.onConfirm}
            size="small"
            variant="danger"
          >
            {props.confirmLabel}
          </ConsoleButton>
        </div>
      </section>
    </div>
  );
}

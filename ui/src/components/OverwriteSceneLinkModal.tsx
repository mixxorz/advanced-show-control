import { useId } from "react";
import { ConsoleButton } from "./ConsoleButton";

/**
 * @cc [owner:mixxorz,label:product;safety] overwrite-modal-explicit-choice
 * The modal MUST describe the exact one-based target scene number/name and source scene name, state
 * that only ASC fade settings are replaced, and offer only cancellation or explicit overwrite;
 * rendering the modal MUST NOT itself invoke either callback.
 */
/**
 * @cc [owner:mixxorz,label:accessibility] overwrite-modal-dialog-obligations
 * The confirmation surface MUST be exposed as a modal dialog programmatically named by its visible
 * decision heading, and both actions MUST be native named buttons wired to their corresponding
 * callback.
 */
export function OverwriteSceneLinkModal(props: {
  targetSceneIndex: number;
  targetSceneName: string;
  sourceSceneName: string;
  onCancel: () => void;
  onOverwrite: () => void;
}) {
  const headingId = useId();

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-6">
      <section
        aria-labelledby={headingId}
        aria-modal="true"
        className="max-w-md rounded-console-panel border border-console-line bg-console-panel p-6 shadow-2xl"
        role="dialog"
      >
        <h2
          className="text-lg font-normal uppercase text-console-primary"
          id={headingId}
        >
          Overwrite Existing Fade Settings?
        </h2>
        <p className="mt-3 text-sm text-console-secondary">
          <span className="text-accent-orange">
            {String(props.targetSceneIndex + 1).padStart(3, "0")}{" "}
            {props.targetSceneName}
          </span>{" "}
          already has fade settings. If you continue, those settings will be
          replaced with the fade settings from{" "}
          <span className="text-accent-orange">{props.sourceSceneName}</span>.
        </p>
        <p className="mt-3 text-sm text-console-secondary">
          This only changes the fade settings saved in Advanced Show Control. No
          changes are made to the actual scene in the console.
        </p>
        <div className="mt-6 flex justify-end gap-3">
          <ConsoleButton
            onClick={props.onCancel}
            size="small"
            variant="secondary"
          >
            Cancel
          </ConsoleButton>
          <ConsoleButton
            onClick={props.onOverwrite}
            size="small"
            variant="danger"
          >
            Overwrite
          </ConsoleButton>
        </div>
      </section>
    </div>
  );
}

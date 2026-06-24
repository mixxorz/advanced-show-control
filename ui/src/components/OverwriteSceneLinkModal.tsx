import { ConsoleButton } from "./ConsoleButton";

export function OverwriteSceneLinkModal(props: {
  targetSceneIndex: number;
  targetSceneName: string;
  sourceSceneName: string;
  onCancel: () => void;
  onOverwrite: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-6">
      <section
        aria-modal="true"
        className="max-w-md rounded-console-panel border border-console-line bg-console-panel p-6 shadow-2xl"
        role="dialog"
      >
        <h2 className="text-lg font-normal uppercase text-console-primary">
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

import { useState } from "react";
import { useAppCommands, useAppState } from "../appHooks";
import { ConfirmModal } from "./ConfirmModal";
import { ConsoleButton } from "./ConsoleButton";

export function CueListManageModal(props: { onClose: () => void }) {
  const { appState } = useAppState();
  const commands = useAppCommands();
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-6">
      <section
        aria-label="Manage Cue Lists"
        aria-modal="true"
        className="flex max-h-[36rem] w-full max-w-xl flex-col overflow-hidden rounded-console-panel border border-console-line bg-console-panel p-6 shadow-2xl"
        role="dialog"
      >
        <div className="border-b border-console-line pb-4">
          <h2 className="text-lg font-normal uppercase text-console-primary">
            Manage Cue Lists
          </h2>
        </div>

        <div className="min-h-0 flex-1 overflow-auto py-4">
          <div className="space-y-3">
            {appState.cueLists.map((cueList) => (
              <div
                className="flex items-center justify-between gap-3 rounded-console-control border border-console-line bg-console-section px-4 py-3"
                key={cueList.id}
              >
                <div>
                  <div className="text-base text-console-primary">
                    {cueList.name}
                  </div>
                  <div className="text-sm text-console-secondary">
                    {cueList.entries.length} cues
                  </div>
                </div>
                <ConsoleButton
                  onClick={() => setPendingDelete(cueList.id)}
                  size="small"
                  variant="ghost-danger"
                >
                  Delete {cueList.name}
                </ConsoleButton>
              </div>
            ))}
          </div>
        </div>

        <div className="flex justify-end gap-3 border-t border-console-line pt-4">
          <ConsoleButton
            onClick={props.onClose}
            size="small"
            variant="secondary"
          >
            Close
          </ConsoleButton>
        </div>
      </section>

      {pendingDelete && (
        <ConfirmModal
          cancelLabel="Cancel"
          confirmLabel="Delete"
          message={`Delete ${appState.cueLists.find((list) => list.id === pendingDelete)?.name ?? "this cue list"}? This only removes the app-managed cue list.`}
          onCancel={() => setPendingDelete(null)}
          onConfirm={() => {
            void commands.deleteCueList?.(pendingDelete);
            setPendingDelete(null);
          }}
          title="Delete Cue List"
        />
      )}
    </div>
  );
}

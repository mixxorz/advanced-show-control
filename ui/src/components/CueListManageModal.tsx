import { useState } from "react";
import { useAppCommands, useAppState } from "../appHooks";
import { ConfirmModal } from "./ConfirmModal";
import { CueListNameModal } from "./CueListNameModal";
import { ConsoleButton } from "./ConsoleButton";

export function CueListManageModal(props: { onClose: () => void }) {
  const { appState } = useAppState();
  const commands = useAppCommands();
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const [pendingRename, setPendingRename] = useState<string | null>(null);
  const [showCreateModal, setShowCreateModal] = useState(false);

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
            <div className="flex justify-end">
              <ConsoleButton
                onClick={() => setShowCreateModal(true)}
                size="small"
              >
                New Cue List
              </ConsoleButton>
            </div>
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
                <div className="flex gap-2">
                  <ConsoleButton
                    onClick={() => setPendingRename(cueList.id)}
                    size="small"
                    variant="secondary"
                  >
                    Rename
                  </ConsoleButton>
                  <ConsoleButton
                    onClick={() => setPendingDelete(cueList.id)}
                    size="small"
                    variant="ghost-danger"
                  >
                    Delete
                  </ConsoleButton>
                </div>
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

      {showCreateModal && (
        <CueListNameModal
          initialName=""
          onCancel={() => setShowCreateModal(false)}
          onSubmit={async (name) => {
            await commands.createCueList?.(name);
            setShowCreateModal(false);
          }}
          submitLabel="Create"
          title="New Cue List"
        />
      )}

      {pendingRename && (
        <CueListNameModal
          initialName={
            appState.cueLists.find((list) => list.id === pendingRename)?.name
          }
          onCancel={() => setPendingRename(null)}
          onSubmit={async (name) => {
            await commands.renameCueList?.(pendingRename, name);
            setPendingRename(null);
          }}
          submitLabel="Rename"
          title="Rename Cue List"
        />
      )}

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

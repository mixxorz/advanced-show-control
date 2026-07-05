import { useState } from "react";
import { useAppCommands, useAppState } from "../appHooks";
import type { CueEntry, CueList } from "../types";
import { ConsoleButton } from "./ConsoleButton";
import { CueListManageModal } from "./CueListManageModal";
import { CueListNameModal } from "./CueListNameModal";
import { Panel } from "./Panel";

export function CueListsTab() {
  const { appState } = useAppState();
  const commands = useAppCommands();
  const activeCueList =
    appState.cueLists.find(
      (cueList) => cueList.id === appState.activeCueListId,
    ) ?? null;
  const [showNewCueListModal, setShowNewCueListModal] = useState(false);
  const [showManageCueListsModal, setShowManageCueListsModal] = useState(false);

  return (
    <div className="grid h-full min-h-0 gap-3 lg:grid-cols-[1fr_2fr]">
      <Panel className="flex min-h-0 flex-col overflow-hidden">
        <div className="border-b border-console-line px-4 py-3">
          <h2 className="text-lg font-normal uppercase text-console-primary">
            Scene Library
          </h2>
        </div>
        <div className="min-h-0 flex-1 overflow-auto p-3">
          {appState.sceneConfigs.map((scene) => (
            <SceneLibraryRow key={scene.internalSceneId} scene={scene} />
          ))}
        </div>
      </Panel>

      <Panel className="flex min-h-0 flex-col overflow-hidden">
        <div className="flex items-center justify-between gap-3 border-b border-console-line px-4 py-3">
          <h2 className="text-lg font-normal uppercase text-console-primary">
            Cue List
          </h2>
          <div className="flex gap-2">
            <ConsoleButton
              onClick={() => setShowNewCueListModal(true)}
              size="small"
            >
              New Cue List
            </ConsoleButton>
            <ConsoleButton
              onClick={() => setShowManageCueListsModal(true)}
              size="small"
            >
              Manage Cue Lists
            </ConsoleButton>
          </div>
        </div>
        <div className="border-b border-console-line-soft px-4 py-2 text-sm text-console-secondary">
          <span className="mr-3">
            Cued: {appState.cuedCueEntryId ?? "None"}
          </span>
          <span className="mr-3">
            Next: {activeCueList?.entries[1]?.id ?? "None"}
          </span>
          <span>Status: {appState.lastCueRecallStatus ?? "idle"}</span>
        </div>
        <div className="min-h-0 flex-1 overflow-auto p-3">
          <CueListPane
            activeCueList={activeCueList}
            onDeleteCueEntry={commands.removeCueEntry}
            onCueEntry={commands.cueEntry}
          />
        </div>
      </Panel>

      {showNewCueListModal && (
        <CueListNameModal
          onCancel={() => setShowNewCueListModal(false)}
          onSubmit={async (name) => {
            await commands.createCueList?.(name);
            setShowNewCueListModal(false);
          }}
          submitLabel="Create"
          title="New Cue List"
        />
      )}

      {showManageCueListsModal && (
        <CueListManageModal onClose={() => setShowManageCueListsModal(false)} />
      )}
    </div>
  );
}

function SceneLibraryRow(props: {
  scene: { internalSceneId: string; sceneName: string };
}) {
  const commands = useAppCommands();
  return (
    <button
      className="mb-2 flex w-full items-center rounded-console-control border border-console-line bg-console-section px-3 py-2 text-left text-console-primary hover:border-console-line-strong hover:bg-console-control"
      draggable
      onDragStart={(event) => {
        event.dataTransfer.setData(
          "application/x-asc-scene-id",
          props.scene.internalSceneId,
        );
      }}
      onClick={() =>
        void commands.addSceneToActiveCueList?.(props.scene.internalSceneId, 0)
      }
      type="button"
    >
      {props.scene.sceneName}
    </button>
  );
}

function CueListPane(props: {
  activeCueList: CueList | null;
  onCueEntry?: (cueEntryId: string | null) => void | Promise<void>;
  onDeleteCueEntry?: (cueEntryId: string) => void | Promise<void>;
}) {
  if (!props.activeCueList) {
    return <p className="text-console-secondary">No active cue list.</p>;
  }

  return (
    <div className="space-y-2">
      {props.activeCueList.entries.map((entry, index) => (
        <CueEntryRow
          entry={entry}
          key={entry.id}
          index={index}
          onCueEntry={props.onCueEntry}
          onDeleteCueEntry={props.onDeleteCueEntry}
        />
      ))}
    </div>
  );
}

function CueEntryRow(props: {
  entry: CueEntry;
  index: number;
  onCueEntry?: (cueEntryId: string | null) => void | Promise<void>;
  onDeleteCueEntry?: (cueEntryId: string) => void | Promise<void>;
}) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-console-control border border-console-line bg-console-section px-3 py-2">
      <button
        className="text-left text-console-primary"
        onClick={() => void props.onCueEntry?.(props.entry.id)}
        type="button"
      >
        Cue {props.index + 1}
      </button>
      <div className="flex items-center gap-2">
        <ConsoleButton
          onClick={() => void props.onDeleteCueEntry?.(props.entry.id)}
          size="small"
          variant="ghost-danger"
        >
          Remove cue {props.index + 1}
        </ConsoleButton>
      </div>
    </div>
  );
}

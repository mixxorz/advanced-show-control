import { useMemo, useState } from "react";
import { useAppCommands, useAppState } from "../appHooks";
import type { CueEntry, CueList, SceneConfig } from "../types";
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
  const activeCueListIndex = useMemo(
    () =>
      appState.cueLists.findIndex(
        (cueList) => cueList.id === activeCueList?.id,
      ),
    [activeCueList?.id, appState.cueLists],
  );
  const [showNewCueListModal, setShowNewCueListModal] = useState(false);
  const [showManageCueListsModal, setShowManageCueListsModal] = useState(false);
  const cuedEntryIndex = activeCueList
    ? activeCueList.entries.findIndex(
        (entry) => entry.id === appState.cuedCueEntryId,
      )
    : -1;
  const cuedEntry =
    cuedEntryIndex >= 0
      ? (activeCueList?.entries[cuedEntryIndex] ?? null)
      : null;
  const nextEntry =
    cuedEntryIndex >= 0
      ? (activeCueList?.entries[cuedEntryIndex + 1] ?? null)
      : null;

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
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-console-line px-4 py-3">
          <div className="flex min-w-0 flex-1 items-center gap-3">
            <h2 className="text-lg font-normal uppercase text-console-primary">
              Cue List
            </h2>
            <label className="flex min-w-0 items-center gap-2 text-sm text-console-secondary">
              Active
              <select
                aria-label="Active cue list"
                className="min-w-0 rounded-console-control border border-console-line bg-console-section px-3 py-2 text-sm text-console-primary outline-none focus:border-console-line-strong"
                onChange={(event) =>
                  void commands.setActiveCueList?.(event.target.value || null)
                }
                value={activeCueList?.id ?? ""}
              >
                <option value="">None</option>
                {appState.cueLists.map((cueList) => (
                  <option key={cueList.id} value={cueList.id}>
                    {cueList.name}
                  </option>
                ))}
              </select>
            </label>
          </div>
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
            Cued:{" "}
            {formatCueEntryLabel(
              cuedEntry,
              cuedEntryIndex,
              appState.sceneConfigs,
            )}
          </span>
          <span className="mr-3">
            Next:{" "}
            {formatCueEntryLabel(
              nextEntry,
              cuedEntryIndex + 1,
              appState.sceneConfigs,
            )}
          </span>
          <span>Status: {appState.lastCueRecallStatus ?? "idle"}</span>
        </div>
        <div className="min-h-0 flex-1 overflow-auto p-3">
          <CueListPane
            activeCueList={activeCueList}
            activeCueListIndex={activeCueListIndex}
            cueLists={appState.cueLists}
            sceneConfigs={appState.sceneConfigs}
            onDropScene={(sceneInternalId, insertIndex) =>
              void commands.addSceneToActiveCueList?.(
                sceneInternalId,
                insertIndex,
              )
            }
            onMoveCueList={(fromIndex, toIndex) => {
              const orderedIds = appState.cueLists.map((cueList) => cueList.id);
              const [movedId] = orderedIds.splice(fromIndex, 1);
              orderedIds.splice(toIndex, 0, movedId);
              void commands.reorderCueLists?.(orderedIds);
            }}
            onCueEntry={commands.cueEntry}
            onDeleteCueEntry={commands.removeCueEntry}
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
  return (
    <div
      className="mb-2 flex w-full items-center rounded-console-control border border-console-line bg-console-section px-3 py-2 text-left text-console-primary hover:border-console-line-strong hover:bg-console-control"
      draggable
      onDragStart={(event) => {
        event.dataTransfer.setData(
          "application/x-asc-scene-id",
          props.scene.internalSceneId,
        );
      }}
    >
      {props.scene.sceneName}
    </div>
  );
}

function CueListPane(props: {
  activeCueList: CueList | null;
  activeCueListIndex: number;
  cueLists: CueList[];
  sceneConfigs: SceneConfig[];
  onDropScene?: (
    sceneInternalId: string,
    insertIndex: number,
  ) => void | Promise<void>;
  onMoveCueList?: (fromIndex: number, toIndex: number) => void | Promise<void>;
  onCueEntry?: (cueEntryId: string | null) => void | Promise<void>;
  onDeleteCueEntry?: (cueEntryId: string) => void | Promise<void>;
}) {
  if (!props.activeCueList) {
    return <p className="text-console-secondary">No active cue list.</p>;
  }

  return (
    <div className="space-y-2">
      <CueListDropZone insertIndex={0} onDropScene={props.onDropScene} />
      {props.activeCueList.entries.map((entry, index) => (
        <div key={entry.id} className="space-y-2">
          <CueEntryRow
            entry={entry}
            index={index}
            onCueEntry={props.onCueEntry}
            onDeleteCueEntry={props.onDeleteCueEntry}
            sceneConfigs={props.sceneConfigs}
          />
          <CueListDropZone
            insertIndex={index + 1}
            onDropScene={props.onDropScene}
          />
        </div>
      ))}
      <div className="flex gap-2 pt-2">
        <ConsoleButton
          disabled={props.activeCueListIndex <= 0}
          onClick={() =>
            void props.onMoveCueList?.(
              props.activeCueListIndex,
              Math.max(0, props.activeCueListIndex - 1),
            )
          }
          size="small"
          variant="secondary"
        >
          Move Up
        </ConsoleButton>
        <ConsoleButton
          disabled={
            props.activeCueListIndex < 0 ||
            props.activeCueListIndex >= props.cueLists.length - 1
          }
          onClick={() =>
            void props.onMoveCueList?.(
              props.activeCueListIndex,
              Math.min(props.cueLists.length - 1, props.activeCueListIndex + 1),
            )
          }
          size="small"
          variant="secondary"
        >
          Move Down
        </ConsoleButton>
      </div>
    </div>
  );
}

function CueListDropZone(props: {
  insertIndex: number;
  onDropScene?: (
    sceneInternalId: string,
    insertIndex: number,
  ) => void | Promise<void>;
}) {
  return (
    <div
      aria-label={`Drop scene at position ${props.insertIndex + 1}`}
      className="rounded-console-control border border-dashed border-console-line px-3 py-2 text-sm text-console-secondary"
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        const sceneInternalId = event.dataTransfer.getData(
          "application/x-asc-scene-id",
        );
        if (sceneInternalId) {
          void props.onDropScene?.(sceneInternalId, props.insertIndex);
        }
      }}
    >
      Drop scene here
    </div>
  );
}

function CueEntryRow(props: {
  entry: CueEntry;
  index: number;
  onCueEntry?: (cueEntryId: string | null) => void | Promise<void>;
  onDeleteCueEntry?: (cueEntryId: string) => void | Promise<void>;
  sceneConfigs: SceneConfig[];
}) {
  const label = formatCueEntryLabel(
    props.entry,
    props.index,
    props.sceneConfigs,
  );

  return (
    <div className="flex items-center justify-between gap-3 rounded-console-control border border-console-line bg-console-section px-3 py-2">
      <button
        className="text-left text-console-primary"
        onClick={() => void props.onCueEntry?.(props.entry.id)}
        type="button"
      >
        {label}
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

function formatCueEntryLabel(
  entry: CueEntry | null,
  index: number,
  sceneConfigs: SceneConfig[],
) {
  if (!entry || index < 0) {
    return "None";
  }

  const scene = sceneConfigs.find(
    (sceneConfig) => sceneConfig.internalSceneId === entry.sceneInternalId,
  );
  return `Cue ${index + 1}: ${scene?.sceneName ?? "Missing scene"}`;
}

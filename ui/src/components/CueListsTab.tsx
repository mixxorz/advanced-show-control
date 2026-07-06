/* eslint-disable react-hooks/refs -- dnd-kit exposes connector refs and drag state through hook return values used in JSX. */
import {
  DndContext,
  PointerSensor,
  type UniqueIdentifier,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragOverEvent,
} from "@dnd-kit/core";
import { CSS } from "@dnd-kit/utilities";
import { useMemo, useState } from "react";
import { useAppCommands, useAppState } from "../appHooks";
import type { CueEntry, CueList, SceneConfig } from "../types";
import { ConsoleButton } from "./ConsoleButton";
import { CueListManageModal } from "./CueListManageModal";
import { CueListNameModal } from "./CueListNameModal";
import { Panel } from "./Panel";
import { SceneListView } from "./SceneList";

type DragData =
  | { kind: "scene"; sceneInternalId: string }
  | { kind: "cueEntry"; cueEntryId: string };

export function CueListsTab() {
  const { appState } = useAppState();
  const commands = useAppCommands();
  const sensors = useSensors(useSensor(PointerSensor));
  const [previewInsertIndex, setPreviewInsertIndex] = useState<number | null>(
    null,
  );
  const [showNewCueListModal, setShowNewCueListModal] = useState(false);
  const [showManageCueListsModal, setShowManageCueListsModal] = useState(false);
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

  function handleDragOver(event: DragOverEvent) {
    setPreviewInsertIndex(cueSlotIndex(event.over?.id));
  }

  function handleDragEnd(event: DragEndEvent) {
    const insertIndex = cueSlotIndex(event.over?.id);
    const dragData = event.active.data.current;
    setPreviewInsertIndex(null);
    if (insertIndex === null || !dragData) return;

    if (isSceneDragData(dragData)) {
      void commands.addSceneToActiveCueList?.(
        dragData.sceneInternalId,
        insertIndex,
      );
      return;
    }

    if (isCueEntryDragData(dragData) && activeCueList) {
      void commands.reorderCueEntries?.(
        reorderEntryIds(
          activeCueList.entries,
          dragData.cueEntryId,
          insertIndex,
        ),
      );
    }
  }

  return (
    <DndContext
      sensors={sensors}
      onDragCancel={() => setPreviewInsertIndex(null)}
      onDragEnd={handleDragEnd}
      onDragOver={handleDragOver}
    >
      <div className="grid h-full min-h-0 gap-3 lg:grid-cols-[1fr_2fr]">
        <SceneListView
          currentScene={appState.currentScene}
          draggableScenes
          onSelectScene={() => undefined}
          scenes={appState.sceneConfigs}
          selectedSceneInternalId={null}
          showRecallControls={false}
          title="Scene Library"
        />

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
              previewInsertIndex={previewInsertIndex}
              sceneConfigs={appState.sceneConfigs}
              onMoveCueList={(fromIndex, toIndex) => {
                const orderedIds = appState.cueLists.map(
                  (cueList) => cueList.id,
                );
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
          <CueListManageModal
            onClose={() => setShowManageCueListsModal(false)}
          />
        )}
      </div>
    </DndContext>
  );
}

function CueListPane(props: {
  activeCueList: CueList | null;
  activeCueListIndex: number;
  cueLists: CueList[];
  previewInsertIndex: number | null;
  sceneConfigs: SceneConfig[];
  onMoveCueList?: (fromIndex: number, toIndex: number) => void | Promise<void>;
  onCueEntry?: (cueEntryId: string | null) => void | Promise<void>;
  onDeleteCueEntry?: (cueEntryId: string) => void | Promise<void>;
}) {
  if (!props.activeCueList) {
    return <p className="text-console-secondary">No active cue list.</p>;
  }
  const activeCueList = props.activeCueList;

  return (
    <div className="space-y-2">
      <CueListDropZone
        insertIndex={0}
        previewed={props.previewInsertIndex === 0}
      />
      {activeCueList.entries.map((entry, index) => (
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
            previewed={props.previewInsertIndex === index + 1}
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

function CueListDropZone(props: { insertIndex: number; previewed: boolean }) {
  const droppable = useDroppable({ id: cueSlotId(props.insertIndex) });

  return (
    <div
      ref={droppable.setNodeRef}
      aria-label={`Drop scene at position ${props.insertIndex + 1}`}
      className={
        props.previewed || droppable.isOver
          ? "rounded-console-control border border-dashed border-accent-orange bg-accent-orange-soft px-3 py-2 text-sm text-console-primary"
          : "rounded-console-control border border-dashed border-console-line px-3 py-2 text-sm text-console-secondary"
      }
    >
      {props.previewed || droppable.isOver ? "Insert here" : "Drop scene here"}
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
  const draggable = useDraggable({
    id: `cueEntry:${props.entry.id}`,
    data: { kind: "cueEntry", cueEntryId: props.entry.id } satisfies DragData,
  });
  const label = formatCueEntryLabel(
    props.entry,
    props.index,
    props.sceneConfigs,
  );

  return (
    <div
      ref={draggable.setNodeRef}
      className="flex items-center justify-between gap-3 rounded-console-control border border-console-line bg-console-section px-3 py-2"
      style={{ transform: CSS.Translate.toString(draggable.transform) }}
    >
      <div className="flex min-w-0 items-center gap-3">
        <span
          aria-label={`Drag cue ${props.index + 1}`}
          className="cursor-grab text-console-secondary active:cursor-grabbing"
          {...draggable.attributes}
          {...draggable.listeners}
        >
          Grip
        </span>
        <button
          className="min-w-0 text-left text-console-primary"
          onClick={() => void props.onCueEntry?.(props.entry.id)}
          type="button"
        >
          {label}
        </button>
      </div>
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

function cueSlotId(insertIndex: number) {
  return `cue-slot:${insertIndex}`;
}

function cueSlotIndex(id: UniqueIdentifier | undefined | null) {
  if (typeof id !== "string" || !id.startsWith("cue-slot:")) return null;
  const insertIndex = Number(id.slice("cue-slot:".length));
  return Number.isFinite(insertIndex) ? insertIndex : null;
}

function isSceneDragData(
  value: unknown,
): value is Extract<DragData, { kind: "scene" }> {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    value.kind === "scene" &&
    "sceneInternalId" in value &&
    typeof value.sceneInternalId === "string"
  );
}

function isCueEntryDragData(
  value: unknown,
): value is Extract<DragData, { kind: "cueEntry" }> {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    value.kind === "cueEntry" &&
    "cueEntryId" in value &&
    typeof value.cueEntryId === "string"
  );
}

function reorderEntryIds(
  entries: CueEntry[],
  cueEntryId: string,
  insertIndex: number,
) {
  const orderedIds = entries.map((entry) => entry.id);
  const currentIndex = orderedIds.indexOf(cueEntryId);
  if (currentIndex < 0) return orderedIds;
  const [movedId] = orderedIds.splice(currentIndex, 1);
  const adjustedInsertIndex =
    currentIndex < insertIndex ? insertIndex - 1 : insertIndex;
  orderedIds.splice(Math.max(0, adjustedInsertIndex), 0, movedId);
  return orderedIds;
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

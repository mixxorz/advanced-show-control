/* eslint-disable react-hooks/refs -- dnd-kit exposes connector refs and drag state through hook return values used in JSX. */
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragOverEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useAppCommands, useAppState } from "../appHooks";
import { formatSceneNumber } from "../format";
import {
  isActionShortcutBlocked,
  shortcutMatchesEvent,
  useKeyboardHandler,
} from "../keyboard";
import type { CueEntry, CueList, SceneConfig } from "../types";
import { ConsoleButton } from "./ConsoleButton";
import { ConsoleIconButton } from "./ConsoleIconButton";
import { CueListManageModal } from "./CueListManageModal";
import { Panel } from "./Panel";
import { SceneListView } from "./SceneList";

type DragData =
  | { kind: "scene"; sceneInternalId: string }
  | { kind: "cueEntry" };

type ActiveDrag =
  | { kind: "scene"; sceneInternalId: string }
  | { kind: "cueEntry"; cueEntryId: string }
  | null;

const CUE_SHORTCUT_PRIORITY = 90;

export function CueListsTab() {
  const { appState } = useAppState();
  const commands = useAppCommands();
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
  );
  const [activeDrag, setActiveDrag] = useState<ActiveDrag>(null);
  const [previewInsertIndex, setPreviewInsertIndex] = useState<number | null>(
    null,
  );
  const [selectedCueEntryId, setSelectedCueEntryId] = useState<string | null>(
    null,
  );
  const [showManageCueListsModal, setShowManageCueListsModal] = useState(false);
  const activeCueList =
    appState.cueLists.find(
      (cueList) => cueList.id === appState.activeCueListId,
    ) ?? null;
  const selectedCueEntry =
    activeCueList?.entries.find((entry) => entry.id === selectedCueEntryId) ??
    null;

  useKeyboardHandler({
    id: "cue-list-cue-shortcut",
    priority: CUE_SHORTCUT_PRIORITY,
    handleKeyDown: (event) => {
      if (isActionShortcutBlocked(event)) {
        return "ignored";
      }
      if (shortcutMatchesEvent(appState.settings.keyboardShortcuts.go, event)) {
        return "handled";
      }
      if (
        !shortcutMatchesEvent(appState.settings.keyboardShortcuts.cue, event)
      ) {
        return "ignored";
      }
      if (event.repeat) return "handled";
      if (selectedCueEntry === null) return "ignored";

      void commands.cueEntry?.(selectedCueEntry.id);
      setSelectedCueEntryId(null);
      return "handled";
    },
  });

  useEffect(() => {
    let canceled = false;
    if (
      selectedCueEntryId !== null &&
      !activeCueList?.entries.some((entry) => entry.id === selectedCueEntryId)
    ) {
      queueMicrotask(() => {
        if (!canceled) {
          setSelectedCueEntryId(null);
        }
      });
    }

    return () => {
      canceled = true;
    };
  }, [activeCueList, selectedCueEntryId]);

  const cuedEntryIndex = activeCueList
    ? activeCueList.entries.findIndex(
        (entry) => entry.id === appState.cuedCueEntryId,
      )
    : -1;
  const cuedEntry =
    cuedEntryIndex >= 0
      ? (activeCueList?.entries[cuedEntryIndex] ?? null)
      : null;
  const cuedSceneInternalId = cuedEntry?.sceneInternalId ?? null;
  function handleDragStart(event: DragStartEvent) {
    const dragData = event.active.data.current;
    if (isSceneDragData(dragData)) {
      setActiveDrag({
        kind: "scene",
        sceneInternalId: dragData.sceneInternalId,
      });
      return;
    }
    if (isCueEntryDragData(dragData)) {
      setActiveDrag({ kind: "cueEntry", cueEntryId: String(event.active.id) });
      return;
    }
    setActiveDrag(null);
  }

  function handleDragOver(event: DragOverEvent) {
    if (!activeCueList) {
      setPreviewInsertIndex(null);
      return;
    }

    const dragData = event.active.data.current;
    if (!isSceneDragData(dragData)) {
      setPreviewInsertIndex(null);
      return;
    }

    const overId = event.over?.id == null ? null : String(event.over.id);
    setPreviewInsertIndex(sceneInsertIndex(activeCueList.entries, overId));
  }

  function handleDragEnd(event: DragEndEvent) {
    const overId = event.over?.id == null ? null : String(event.over.id);
    const dragData = event.active.data.current;
    setActiveDrag(null);
    setPreviewInsertIndex(null);
    if (!overId || !dragData) return;

    if (isSceneDragData(dragData) && activeCueList) {
      const insertIndex = sceneInsertIndex(activeCueList.entries, overId);
      if (insertIndex === null) return;

      void commands.addSceneToActiveCueList?.(
        dragData.sceneInternalId,
        insertIndex,
      );
      return;
    }

    if (isCueEntryDragData(dragData) && activeCueList) {
      const activeId = String(event.active.id);
      if (activeId === overId) return;

      const oldIndex = activeCueList.entries.findIndex(
        (entry) => entry.id === activeId,
      );
      const newIndex = activeCueList.entries.findIndex(
        (entry) => entry.id === overId,
      );
      if (oldIndex < 0 || newIndex < 0) return;

      void commands.reorderCueEntries?.(
        arrayMove(activeCueList.entries, oldIndex, newIndex).map(
          (entry) => entry.id,
        ),
      );
    }
  }

  return (
    <DndContext
      sensors={sensors}
      onDragCancel={() => {
        setActiveDrag(null);
        setPreviewInsertIndex(null);
      }}
      onDragEnd={handleDragEnd}
      onDragOver={handleDragOver}
      onDragStart={handleDragStart}
    >
      <div className="grid h-full min-h-0 gap-3 lg:grid-cols-[23rem_1fr]">
        <SceneListView
          currentScene={appState.currentScene}
          cuedSceneInternalId={cuedSceneInternalId}
          dragOverlayOnly
          draggableScenes
          onSelectScene={() => undefined}
          scenes={appState.sceneConfigs}
          selectedSceneInternalId={null}
          title="Scene library"
        />

        <div className="flex min-h-0 flex-col gap-3">
          <Panel className="flex min-h-0 flex-1 flex-col overflow-hidden">
            <div className="flex flex-wrap items-center justify-between gap-3 border-b border-console-line px-4 py-3">
              <h2 className="min-w-0 truncate font-mono text-lg font-normal text-accent-orange">
                {activeCueList?.name ?? "No active cue list"}
              </h2>
              <div className="flex gap-2">
                <ConsoleButton
                  disabled={selectedCueEntry === null}
                  onClick={() => {
                    void commands.cueEntry?.(selectedCueEntry?.id ?? null);
                    setSelectedCueEntryId(null);
                  }}
                  size="small"
                  variant="ghost-primary"
                >
                  Cue
                </ConsoleButton>
                <ConsoleButton
                  onClick={() => setShowManageCueListsModal(true)}
                  size="small"
                >
                  Manage Cue Lists
                </ConsoleButton>
              </div>
            </div>
            <div className="min-h-0 flex-1 overflow-auto">
              <div className="grid grid-cols-[1.25rem_1fr_4rem_3rem] border-b border-console-line-soft py-2 pr-2 pl-0 text-sm uppercase tracking-[0.08em] text-console-secondary">
                <span aria-hidden="true" />
                <span className="translate-y-0.5">Scene Name</span>
                <span className="translate-y-0.5 text-right">#</span>
                <span aria-hidden="true" />
              </div>
              <CueListPane
                activeCueList={activeCueList}
                draggingScene={activeDrag?.kind === "scene"}
                draggingSceneConfig={
                  activeDrag?.kind === "scene"
                    ? (appState.sceneConfigs.find(
                        (scene) =>
                          scene.internalSceneId === activeDrag.sceneInternalId,
                      ) ?? null)
                    : null
                }
                previewInsertIndex={previewInsertIndex}
                sceneConfigs={appState.sceneConfigs}
                currentScene={appState.currentScene}
                cuedCueEntryId={appState.cuedCueEntryId}
                selectedCueEntryId={selectedCueEntryId}
                onSelectCueEntry={setSelectedCueEntryId}
                onCueEntry={(cueEntryId) => {
                  void commands.cueEntry?.(cueEntryId);
                  setSelectedCueEntryId(null);
                }}
                onDeleteCueEntry={commands.removeCueEntry}
              />
            </div>
          </Panel>
        </div>

        {showManageCueListsModal && (
          <CueListManageModal
            onClose={() => setShowManageCueListsModal(false)}
          />
        )}
      </div>
      <DragOverlay dropAnimation={null}>
        {activeDrag?.kind === "scene" ? (
          <SceneDragOverlay
            scene={
              appState.sceneConfigs.find(
                (scene) => scene.internalSceneId === activeDrag.sceneInternalId,
              ) ?? null
            }
          />
        ) : activeDrag?.kind === "cueEntry" ? (
          <CueEntryDragOverlay
            entry={
              activeCueList?.entries.find(
                (entry) => entry.id === activeDrag.cueEntryId,
              ) ?? null
            }
            index={
              activeCueList?.entries.findIndex(
                (entry) => entry.id === activeDrag.cueEntryId,
              ) ?? -1
            }
            sceneConfigs={appState.sceneConfigs}
          />
        ) : null}
      </DragOverlay>
    </DndContext>
  );
}

function CueEntryDragOverlay(props: {
  entry: CueEntry | null;
  index: number;
  sceneConfigs: SceneConfig[];
}) {
  if (!props.entry) return null;

  return (
    <div className="grid w-[calc(100vw-30rem)] min-w-[24rem] max-w-[calc(100vw-30rem)] grid-cols-[1fr_4rem_3rem] items-center border border-accent-orange bg-console-section py-1.5 pr-2 pl-3 text-left shadow-2xl">
      <div className="min-w-0 truncate text-base font-normal text-console-primary">
        {formatCueEntrySceneName(props.entry, props.sceneConfigs)}
      </div>
      <span className="text-right font-mono text-base text-console-secondary">
        {formatCueEntrySceneNumber(props.entry, props.sceneConfigs)}
      </span>
      <span className="grid h-8 w-8 place-items-center justify-self-end">
        <Trash2 aria-hidden="true" className="h-4 w-4 text-status-danger" />
      </span>
    </div>
  );
}

function SceneDragOverlay(props: { scene: SceneConfig | null }) {
  if (!props.scene) return null;

  return (
    <div className="grid w-[23rem] grid-cols-[1.25rem_3rem_1fr_4rem] items-center rounded-console-control border border-accent-orange bg-console-section px-3 py-2 text-left shadow-2xl">
      <span aria-hidden="true" />
      <span className="font-mono text-base text-console-secondary">
        {props.scene.sceneIndex == null
          ? "---"
          : String(props.scene.sceneIndex + 1).padStart(3, "0")}
      </span>
      <span className="truncate text-base text-console-primary">
        {props.scene.sceneName}
      </span>
      <span className="text-right font-mono text-base text-console-secondary">
        {props.scene.durationMs === 0
          ? "Cut"
          : `${(props.scene.durationMs / 1000).toFixed(1)}s`}
      </span>
    </div>
  );
}

function CueListPane(props: {
  activeCueList: CueList | null;
  draggingScene: boolean;
  draggingSceneConfig: SceneConfig | null;
  previewInsertIndex: number | null;
  sceneConfigs: SceneConfig[];
  currentScene: { index: number; name: string } | null;
  cuedCueEntryId: string | null;
  selectedCueEntryId: string | null;
  onSelectCueEntry?: (cueEntryId: string | null) => void;
  onCueEntry?: (cueEntryId: string) => void | Promise<void>;
  onDeleteCueEntry?: (cueEntryId: string) => void | Promise<void>;
}) {
  const listDroppable = useDroppable({ id: sceneDropZoneId });
  if (!props.activeCueList) {
    return <p className="text-console-secondary">No active cue list.</p>;
  }
  const activeCueList = props.activeCueList;
  const appendPreview =
    props.draggingScene &&
    (props.previewInsertIndex === activeCueList.entries.length ||
      listDroppable.isOver);

  return (
    <div ref={listDroppable.setNodeRef} className="min-h-full">
      <SortableContext
        items={activeCueList.entries.map((entry) => entry.id)}
        strategy={verticalListSortingStrategy}
      >
        {activeCueList.entries.map((entry, index) => (
          <CueEntryRow
            entry={entry}
            index={index}
            previewSceneInsert={props.previewInsertIndex === index}
            previewSceneConfig={props.draggingSceneConfig}
            currentScene={props.currentScene}
            cued={entry.id === props.cuedCueEntryId}
            selected={entry.id === props.selectedCueEntryId}
            key={entry.id}
            onSelectCueEntry={props.onSelectCueEntry}
            onCueEntry={props.onCueEntry}
            onDeleteCueEntry={props.onDeleteCueEntry}
            sceneConfigs={props.sceneConfigs}
          />
        ))}
      </SortableContext>
      {appendPreview ? (
        <SceneInsertPreview scene={props.draggingSceneConfig} />
      ) : null}
    </div>
  );
}

function CueEntryRow(props: {
  entry: CueEntry;
  index: number;
  previewSceneInsert: boolean;
  previewSceneConfig: SceneConfig | null;
  currentScene: { index: number; name: string } | null;
  cued: boolean;
  selected: boolean;
  onSelectCueEntry?: (cueEntryId: string | null) => void;
  onCueEntry?: (cueEntryId: string) => void | Promise<void>;
  onDeleteCueEntry?: (cueEntryId: string) => void | Promise<void>;
  sceneConfigs: SceneConfig[];
}) {
  const sortable = useSortable({
    id: props.entry.id,
    data: { kind: "cueEntry" } satisfies DragData,
  });
  const rowLabel = formatCueEntrySceneName(props.entry, props.sceneConfigs);
  const scene = props.sceneConfigs.find(
    (sceneConfig) =>
      sceneConfig.internalSceneId === props.entry.sceneInternalId,
  );
  const current =
    !!scene &&
    props.currentScene?.index === scene.sceneIndex &&
    props.currentScene.name === scene.sceneName;
  const textClass = current
    ? "text-status-current"
    : props.cued
      ? "text-status-cued"
      : props.selected
        ? "text-console-primary"
        : scene
          ? "text-console-primary"
          : "text-status-warning";
  const stateClass = current
    ? "text-status-current"
    : props.cued
      ? "text-status-cued"
      : props.selected
        ? "text-accent-orange"
        : scene
          ? "text-console-secondary"
          : "text-status-warning";
  const leftBorderClass = current
    ? "border-l-status-current"
    : props.cued
      ? "border-l-status-cued"
      : props.selected
        ? "border-l-accent-orange"
        : scene
          ? "border-l-transparent"
          : "border-l-status-warning";
  const showIndicator = current || props.cued || props.selected || !scene;

  return (
    <>
      {props.previewSceneInsert ? (
        <SceneInsertPreview scene={props.previewSceneConfig} />
      ) : null}
      <div
        ref={sortable.setNodeRef}
        className={
          props.selected
            ? `grid w-full grid-cols-[1.25rem_1fr_4rem_3rem] items-center border border-accent-orange-active border-l-[3px] ${leftBorderClass} bg-accent-orange-soft py-1.5 pr-2 pl-0 text-left`
            : `grid w-full grid-cols-[1.25rem_1fr_4rem_3rem] items-center border border-transparent border-b-console-line-soft/60 border-l-[3px] ${leftBorderClass} py-1.5 pr-2 pl-0 text-left hover:bg-console-section`
        }
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            props.onSelectCueEntry?.(props.entry.id);
          }
        }}
        onClick={() => props.onSelectCueEntry?.(props.entry.id)}
        onDoubleClick={() => void props.onCueEntry?.(props.entry.id)}
        style={{
          opacity: sortable.isDragging ? 0.55 : 1,
          transform: CSS.Transform.toString(
            sortable.transform ? { ...sortable.transform, x: 0 } : null,
          ),
          transition: sortable.transition,
          zIndex: sortable.isDragging ? 1 : undefined,
        }}
        {...sortable.attributes}
        {...sortable.listeners}
      >
        <span className="flex justify-start overflow-visible">
          {showIndicator ? (
            <svg
              aria-hidden="true"
              className={`h-4 w-[0.7rem] fill-current ${stateClass}`}
              viewBox="0 0 7 10"
            >
              <polygon points="0,0 7,5 0,10" />
            </svg>
          ) : null}
        </span>
        <div className={`min-w-0 truncate text-base font-normal ${textClass}`}>
          {rowLabel}
        </div>
        <span className={`text-right font-mono text-base ${textClass}`}>
          {formatCueEntrySceneNumber(props.entry, props.sceneConfigs)}
        </span>
        <div className="flex justify-end">
          <ConsoleIconButton
            aria-label={`Remove cue ${props.index + 1}`}
            onClick={(event) => {
              event.stopPropagation();
              void props.onDeleteCueEntry?.(props.entry.id);
            }}
            onPointerDown={(event) => event.stopPropagation()}
            size="small"
            variant="ghost-danger"
          >
            <Trash2 aria-hidden="true" className="h-4 w-4" />
          </ConsoleIconButton>
        </div>
      </div>
    </>
  );
}

function formatCueEntrySceneName(entry: CueEntry, sceneConfigs: SceneConfig[]) {
  const scene = sceneConfigs.find(
    (sceneConfig) => sceneConfig.internalSceneId === entry.sceneInternalId,
  );
  return scene?.sceneName ?? "Missing scene";
}

function formatCueEntrySceneNumber(
  entry: CueEntry,
  sceneConfigs: SceneConfig[],
) {
  const scene = sceneConfigs.find(
    (sceneConfig) => sceneConfig.internalSceneId === entry.sceneInternalId,
  );
  return formatSceneNumber(scene?.sceneIndex ?? null);
}

function SceneInsertPreview(props: { scene: SceneConfig | null }) {
  return (
    <div className="grid w-full grid-cols-[1fr_4rem_3rem] items-center border border-dashed border-accent-orange bg-accent-orange-soft py-1.5 pr-2 pl-3 text-left opacity-80">
      <div className="min-w-0 truncate text-base font-normal text-console-primary">
        {props.scene?.sceneName ?? "Scene"}
      </div>
      <span className="text-right font-mono text-base text-console-secondary">
        {formatSceneNumber(props.scene?.sceneIndex ?? null)}
      </span>
      <span className="grid h-8 w-8 place-items-center justify-self-end">
        <Trash2
          aria-hidden="true"
          className="h-4 w-4 text-status-danger opacity-60"
        />
      </span>
    </div>
  );
}

const sceneDropZoneId = "cue-scene-drop-zone";

function sceneInsertIndex(entries: CueEntry[], overId: string | null) {
  if (!overId) return null;
  if (overId === sceneDropZoneId) return entries.length;
  const entryIndex = entries.findIndex((entry) => entry.id === overId);
  return entryIndex >= 0 ? entryIndex : null;
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
    value.kind === "cueEntry"
  );
}

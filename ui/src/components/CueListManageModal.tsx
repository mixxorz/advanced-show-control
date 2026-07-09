/* eslint-disable react-hooks/refs -- dnd-kit exposes connector refs and drag state through hook return values used in JSX. */
import {
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { GripVertical, SquarePen, Trash2 } from "lucide-react";
import { useState } from "react";
import { useAppCommands, useAppState } from "../appHooks";
import type { CueList } from "../types";
import { ConfirmModal } from "./ConfirmModal";
import { ConsoleIconButton } from "./ConsoleIconButton";
import { CueListNameModal } from "./CueListNameModal";
import { ConsoleButton } from "./ConsoleButton";

export function CueListManageModal(props: { onClose: () => void }) {
  const { appState } = useAppState();
  const commands = useAppCommands();
  const sensors = useSensors(useSensor(PointerSensor));
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const [pendingRename, setPendingRename] = useState<string | null>(null);
  const [showCreateModal, setShowCreateModal] = useState(false);

  function handleDragEnd(event: DragEndEvent) {
    const activeId = String(event.active.id);
    const overId = event.over?.id == null ? null : String(event.over.id);
    if (!overId || activeId === overId) return;

    const oldIndex = appState.cueLists.findIndex(
      (cueList) => cueList.id === activeId,
    );
    const newIndex = appState.cueLists.findIndex(
      (cueList) => cueList.id === overId,
    );
    if (oldIndex < 0 || newIndex < 0) return;

    void commands.reorderCueLists?.(
      arrayMove(appState.cueLists, oldIndex, newIndex).map(
        (cueList) => cueList.id,
      ),
    );
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/75 p-6 font-ui text-console-primary">
      <DndContext sensors={sensors} onDragEnd={handleDragEnd}>
        <section
          aria-label="Manage Cue Lists"
          aria-modal="true"
          className="grid h-[min(70vh,36rem)] max-h-full w-full max-w-xl grid-rows-[auto_1fr] gap-5 overflow-hidden rounded-console-panel border border-console-line bg-console-panel/95 px-6 py-6 shadow-2xl"
          role="dialog"
        >
          <div className="flex items-start justify-between gap-6 border-b border-console-line pb-4">
            <div className="min-w-0">
              <h1 className="text-lg font-normal uppercase text-console-primary">
                Cue lists
              </h1>
            </div>

            <div className="flex items-center gap-3">
              <ConsoleButton
                onClick={() => setShowCreateModal(true)}
                size="small"
              >
                New Cue List
              </ConsoleButton>
              <button
                aria-label="Close manage cue lists modal"
                className="relative h-7 w-7 text-console-secondary hover:text-console-primary"
                onClick={props.onClose}
                type="button"
              >
                <span className="absolute top-1/2 left-1/2 h-6 w-0.5 -translate-x-1/2 -translate-y-1/2 rotate-45 rounded-full bg-current" />
                <span className="absolute top-1/2 left-1/2 h-6 w-0.5 -translate-x-1/2 -translate-y-1/2 -rotate-45 rounded-full bg-current" />
              </button>
            </div>
          </div>

          <div className="min-h-0 overflow-x-hidden overflow-y-auto">
            <SortableContext
              items={appState.cueLists.map((cueList) => cueList.id)}
              strategy={verticalListSortingStrategy}
            >
              <div className="grid content-start gap-2">
                {appState.cueLists.map((cueList) => (
                  <CueListRow
                    active={cueList.id === appState.activeCueListId}
                    cueList={cueList}
                    key={cueList.id}
                    onDelete={() => setPendingDelete(cueList.id)}
                    onRename={() => setPendingRename(cueList.id)}
                    onSelect={() => {
                      void commands.setActiveCueList?.(cueList.id);
                      props.onClose();
                    }}
                  />
                ))}
              </div>
            </SortableContext>
          </div>
        </section>
      </DndContext>

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
          message={
            <>
              Delete{" "}
              <span className="font-medium text-console-primary">
                {appState.cueLists.find((list) => list.id === pendingDelete)
                  ?.name ?? "this cue list"}
              </span>
              ? This only removes the app-managed cue list.
            </>
          }
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

function CueListRow(props: {
  active: boolean;
  cueList: CueList;
  onDelete: () => void;
  onRename: () => void;
  onSelect: () => void;
}) {
  const sortable = useSortable({ id: props.cueList.id });

  return (
    <div
      ref={sortable.setNodeRef}
      className={
        props.active
          ? "grid w-full gap-3 rounded-console-control border border-accent-orange bg-accent-orange-soft py-2.5 pr-2 pl-3 text-left md:grid-cols-[auto_1fr_auto] md:items-center"
          : "grid w-full gap-3 rounded-console-control border border-console-line bg-console-section/70 py-2.5 pr-2 pl-3 text-left hover:border-console-line-strong hover:bg-console-control/70 md:grid-cols-[auto_1fr_auto] md:items-center"
      }
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          props.onSelect();
        }
      }}
      onClick={props.onSelect}
      style={{
        opacity: sortable.isDragging ? 0.55 : 1,
        transform: CSS.Transform.toString(
          sortable.transform ? { ...sortable.transform, x: 0 } : null,
        ),
        transition: sortable.transition,
        zIndex: sortable.isDragging ? 1 : undefined,
      }}
      role="button"
      tabIndex={0}
    >
      <span
        aria-label={`Drag ${props.cueList.name}`}
        className="cursor-grab text-console-secondary active:cursor-grabbing"
        onClick={(event) => event.stopPropagation()}
        {...sortable.attributes}
        {...sortable.listeners}
      >
        <GripVertical aria-hidden="true" className="h-5 w-5" />
      </span>
      <div className="min-w-0 truncate text-base font-normal text-console-primary">
        {props.cueList.name}
      </div>
      <div className="flex flex-wrap gap-1 md:justify-self-end">
        <ConsoleIconButton
          aria-label={`Rename ${props.cueList.name}`}
          onClick={(event) => {
            event.stopPropagation();
            props.onRename();
          }}
          size="small"
          variant="secondary"
        >
          <SquarePen aria-hidden="true" className="h-4 w-4" />
        </ConsoleIconButton>
        <ConsoleIconButton
          aria-label={`Delete ${props.cueList.name}`}
          onClick={(event) => {
            event.stopPropagation();
            props.onDelete();
          }}
          size="small"
          variant="ghost-danger"
        >
          <Trash2 aria-hidden="true" className="h-4 w-4" />
        </ConsoleIconButton>
      </div>
    </div>
  );
}

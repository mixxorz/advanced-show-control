/* eslint-disable react-hooks/refs -- dnd-kit exposes connector refs and drag state through hook return values used in JSX. */
import { useDraggable } from "@dnd-kit/core";
import type { Data } from "@dnd-kit/core";
import { CSS } from "@dnd-kit/utilities";
import type { SceneConfig, SceneSummary } from "../types";
import { formatSceneDurationSummary, formatSceneNumber } from "../format";

export function SceneListRow(props: {
  currentScene: SceneSummary | null;
  cued: boolean;
  scene: SceneConfig;
  selected: boolean;
  dragId?: string;
  dragData?: Data;
  dragOverlayOnly?: boolean;
  onSelect: () => void;
}) {
  const draggable = useDraggable({
    id: props.dragId ?? `disabled-${props.scene.internalSceneId}`,
    data: props.dragData,
    disabled: !props.dragId,
  });
  const unlinked = props.scene.sceneIndex === null;
  const current =
    props.currentScene?.index === props.scene.sceneIndex &&
    props.currentScene.name === props.scene.sceneName;
  const textClass = unlinked
    ? "text-status-warning"
    : current
      ? "text-status-current"
      : props.cued
        ? "text-status-cued"
        : props.selected
          ? "text-console-primary"
          : "text-console-secondary";
  const stateClass = current
    ? "text-status-current"
    : props.cued
      ? "text-status-cued"
      : unlinked
        ? "text-status-warning"
        : props.selected
          ? "text-accent-orange"
          : "text-console-secondary";
  const leftBorderClass = !props.selected
    ? "border-l-[3px] border-l-transparent"
    : current
      ? "border-l-[3px] border-l-status-current"
      : props.cued
        ? "border-l-[3px] border-l-status-cued"
        : unlinked
          ? "border-l-[3px] border-l-status-warning"
          : "border-l-[3px] border-l-accent-orange";
  const durationClass = unlinked
    ? "text-right font-mono text-status-warning"
    : current
      ? "text-right font-mono text-status-current"
      : props.cued
        ? "text-right font-mono text-status-cued"
        : props.selected
          ? "text-right font-mono text-console-primary"
          : "text-right font-mono text-console-secondary";
  const showIndicator = current || props.cued || props.selected;

  return (
    <button
      className={
        props.selected
          ? `grid w-full grid-cols-[1.25rem_3rem_1fr_4rem] items-center border border-accent-orange-active ${leftBorderClass} bg-accent-orange-soft py-1.5 pr-3 pl-0 text-left text-console-primary`
          : `grid w-full grid-cols-[1.25rem_3rem_1fr_4rem] items-center border border-transparent border-b-console-line-soft/60 ${leftBorderClass} py-1.5 pr-3 pl-0 text-left text-console-secondary hover:bg-console-section hover:text-console-primary`
      }
      ref={draggable.setNodeRef}
      style={{
        opacity: props.dragOverlayOnly && draggable.isDragging ? 0.45 : 1,
        transform: props.dragOverlayOnly
          ? undefined
          : CSS.Translate.toString(draggable.transform),
      }}
      {...draggable.attributes}
      {...draggable.listeners}
      onClick={props.onSelect}
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
      <span className={`font-mono text-base ${textClass}`}>
        {formatSceneNumber(props.scene.sceneIndex)}
      </span>
      <span className={`truncate text-base ${textClass}`}>
        {props.scene.sceneName}
      </span>
      <span className={`text-base ${durationClass}`}>
        {formatSceneDurationSummary(props.scene.durationMs)}
      </span>
    </button>
  );
}

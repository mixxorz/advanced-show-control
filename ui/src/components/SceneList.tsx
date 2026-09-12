import { useAppCommands, useAppState } from "../appHooks";
import type { SceneConfig, SceneSummary } from "../types";
import { Panel } from "./Panel";
import { SceneListRow } from "./SceneListRow";

function duplicateSceneNames(scenes: SceneConfig[]): string[] {
  const counts = new Map<string, number>();
  for (const scene of scenes)
    counts.set(scene.sceneName, (counts.get(scene.sceneName) ?? 0) + 1);
  return [...counts.entries()]
    .filter(([, count]) => count > 1)
    .map(([name]) => name)
    .sort((a, b) => a.localeCompare(b));
}

/**
 * @cc [owner:mixxorz,label:product;accessibility] scene-list-selection-and-fallbacks
 * Each rendered scene MUST remain a named selectable button whose callback receives that scene's
 * exact internal ID. An empty collection MUST show `No scenes loaded.`, and an omitted title MUST
 * fall back to `Scene library`.
 */
/**
 * @cc [owner:mixxorz,label:product] scene-list-duplicate-warning
 * The duplicate warning MUST list each case-sensitive scene name occurring more than once exactly
 * once, sorted by locale, and MUST be absent when all names are unique.
 */
export function SceneListView(props: {
  currentScene: SceneSummary | null;
  cuedSceneInternalId?: string | null;
  dragOverlayOnly?: boolean;
  draggableScenes?: boolean;
  selectedSceneInternalId?: string | null;
  scenes: SceneConfig[];
  title?: string;
  onSelectScene: (internalSceneId: string) => void;
}) {
  const duplicateNames = duplicateSceneNames(props.scenes);

  return (
    <Panel className="flex min-h-0 flex-col overflow-hidden">
      <div className="flex items-center justify-between gap-3 border-b border-console-line px-4 py-3">
        <h2 className="text-lg font-normal uppercase text-console-primary">
          {props.title ?? "Scene library"}
        </h2>
      </div>
      <div className="grid grid-cols-[1.25rem_3rem_1fr_4rem] border-b border-console-line-soft py-2 pr-3 pl-0 text-sm uppercase tracking-[0.08em] text-console-secondary">
        <span aria-hidden="true" />
        <span className="translate-y-0.5">#</span>
        <span className="translate-y-0.5">Scene Name</span>
        <span className="translate-y-0.5 text-right">X-Fade</span>
      </div>
      {duplicateNames.length > 0 ? (
        <div className="border-b border-status-warning bg-console-section px-3 py-2 text-sm text-status-warning">
          Duplicate scene names: {duplicateNames.join(", ")}
        </div>
      ) : null}
      <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto">
        {props.scenes.length === 0 ? (
          <p className="p-4 text-sm text-console-muted">No scenes loaded.</p>
        ) : (
          props.scenes.map((scene) => (
            <SceneListRow
              currentScene={props.currentScene}
              cued={scene.internalSceneId === props.cuedSceneInternalId}
              dragData={
                props.draggableScenes
                  ? {
                      kind: "scene",
                      sceneInternalId: scene.internalSceneId,
                    }
                  : undefined
              }
              dragId={
                props.draggableScenes
                  ? `scene:${scene.internalSceneId}`
                  : undefined
              }
              dragOverlayOnly={props.dragOverlayOnly}
              key={scene.internalSceneId}
              onSelect={() => props.onSelectScene(scene.internalSceneId)}
              scene={scene}
              selected={scene.internalSceneId === props.selectedSceneInternalId}
            />
          ))
        )}
      </div>
    </Panel>
  );
}

/**
 * @cc [owner:mixxorz,label:architecture] projected-scene-list-command-boundary
 * The production list MUST render projected scene configs, current scene, and selected internal ID,
 * and selection MUST be requested through `commands.selectScene`; it MUST NOT mutate local scene
 * selection state.
 */
export function SceneList() {
  const { appState } = useAppState();
  const commands = useAppCommands();

  return (
    <SceneListView
      currentScene={appState.currentScene}
      selectedSceneInternalId={appState.selectedSceneInternalId ?? null}
      onSelectScene={commands.selectScene}
      scenes={appState.sceneConfigs}
    />
  );
}

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

export function SceneList(props: {
  currentScene: SceneSummary | null;
  scenes: SceneConfig[];
  selectedSceneId: string | null;
  onSelectScene: (sceneId: string) => void;
}) {
  const duplicateNames = duplicateSceneNames(props.scenes);

  return (
    <Panel className="flex min-h-0 flex-col overflow-hidden">
      <div className="border-b border-console-line px-4 py-3">
        <h2 className="text-base font-semibold uppercase tracking-[0.08em] text-console-primary">
          Scene List
        </h2>
      </div>
      <div className="grid grid-cols-[4rem_1fr_4rem] border-b border-console-line-soft px-3 py-2 text-xs uppercase tracking-[0.08em] text-console-secondary">
        <span>#</span>
        <span>Scene Name</span>
        <span className="text-right">X-Fade</span>
      </div>
      {duplicateNames.length > 0 ? (
        <div className="border-b border-status-warning bg-console-section px-3 py-2 text-sm text-status-warning">
          Duplicate scene names: {duplicateNames.join(", ")}
        </div>
      ) : null}
      <div className="min-h-0 flex-1 overflow-auto">
        {props.scenes.length === 0 ? (
          <p className="p-4 text-sm text-console-muted">No scenes loaded.</p>
        ) : (
          props.scenes.map((scene) => (
            <SceneListRow
              currentScene={props.currentScene}
              key={scene.sceneId}
              onSelect={() => props.onSelectScene(scene.sceneId)}
              scene={scene}
              selected={scene.sceneId === props.selectedSceneId}
            />
          ))
        )}
      </div>
    </Panel>
  );
}

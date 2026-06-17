import { useAppCommands, useAppState } from "../appHooks";
import type { SceneConfig, SceneSummary } from "../types";
import { ConsoleButton } from "./ConsoleButton";
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

export function SceneListView(props: {
  currentScene: SceneSummary | null;
  cuedSceneId?: string | null;
  scenes: SceneConfig[];
  selectedSceneId: string | null;
  onSelectScene: (sceneId: string) => void;
  onRecallScene?: (sceneId: string) => void;
}) {
  const duplicateNames = duplicateSceneNames(props.scenes);
  const currentIndex = props.scenes.findIndex(
    (scene) =>
      props.currentScene?.index === scene.sceneIndex &&
      props.currentScene.name === scene.sceneName,
  );
  const canRecallPrevious = currentIndex > 0;
  const canRecallNext =
    currentIndex >= 0 && currentIndex < props.scenes.length - 1;

  function recallPreviousScene() {
    if (!canRecallPrevious) return;
    props.onRecallScene?.(props.scenes[currentIndex - 1].sceneId);
  }

  function recallNextScene() {
    if (!canRecallNext) return;
    props.onRecallScene?.(props.scenes[currentIndex + 1].sceneId);
  }

  return (
    <Panel className="flex min-h-0 flex-col overflow-hidden">
      <div className="flex items-center justify-between gap-3 border-b border-console-line px-4 py-3">
        <h2 className="text-lg font-normal uppercase text-console-primary">
          Scene List
        </h2>
        <div className="flex gap-2">
          <ConsoleButton
            disabled={!canRecallPrevious || !props.onRecallScene}
            onClick={recallPreviousScene}
            size="small"
          >
            Prev
          </ConsoleButton>
          <ConsoleButton
            disabled={!canRecallNext || !props.onRecallScene}
            onClick={recallNextScene}
            size="small"
          >
            Next
          </ConsoleButton>
        </div>
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
      <div className="min-h-0 flex-1 overflow-auto">
        {props.scenes.length === 0 ? (
          <p className="p-4 text-sm text-console-muted">No scenes loaded.</p>
        ) : (
          props.scenes.map((scene) => (
            <SceneListRow
              currentScene={props.currentScene}
              cued={scene.sceneId === props.cuedSceneId}
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

export function SceneList() {
  const { appState } = useAppState();
  const commands = useAppCommands();

  return (
    <SceneListView
      currentScene={appState.currentScene}
      cuedSceneId={appState.cuedSceneId ?? null}
      onRecallScene={commands.recallScene}
      onSelectScene={commands.selectScene}
      scenes={appState.sceneConfigs}
      selectedSceneId={appState.selectedSceneId}
    />
  );
}

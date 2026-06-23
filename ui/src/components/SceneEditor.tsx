import { useAppState } from "../appHooks";
import { ChannelScopeGrid } from "./ChannelScopeGrid";
import { EmptySceneSelection } from "./EmptySceneSelection";
import { SelectedSceneHeader } from "./SelectedSceneHeader";
import { UnlinkedSceneControls } from "./UnlinkedSceneControls";

export function SceneEditor() {
  const { appState } = useAppState();
  const selected = appState.sceneConfigs.find(
    (scene) => scene.internalSceneId === appState.selectedSceneInternalId,
  );

  if (!selected) {
    return <EmptySceneSelection />;
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <SelectedSceneHeader scene={selected} />
      {selected.sceneIndex === null ? (
        <UnlinkedSceneControls
          existingConfigs={appState.sceneConfigs}
          lv1Scenes={appState.scenes}
          scene={selected}
        />
      ) : null}
      <ChannelScopeGrid scene={selected} />
    </div>
  );
}

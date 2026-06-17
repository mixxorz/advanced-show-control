import { useAppState } from "../appHooks";
import { ChannelScopeGrid } from "./ChannelScopeGrid";
import { EmptySceneSelection } from "./EmptySceneSelection";
import { SelectedSceneHeader } from "./SelectedSceneHeader";

export function SceneEditor() {
  const { appState } = useAppState();
  const selected = appState.sceneConfigs.find(
    (scene) => scene.sceneId === appState.selectedSceneId,
  );

  if (!selected) {
    return <EmptySceneSelection />;
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <SelectedSceneHeader scene={selected} />
      <ChannelScopeGrid scene={selected} />
    </div>
  );
}

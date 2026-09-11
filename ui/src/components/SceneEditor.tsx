import { useAppState } from "../appHooks";
import { ChannelScopeGrid } from "./ChannelScopeGrid";
import { EmptySceneSelection } from "./EmptySceneSelection";
import { LinkSceneControls } from "./LinkSceneControls";
import { SelectedSceneHeader } from "./SelectedSceneHeader";

/**
 * @cc [owner:mixxorz,label:product] selected-scene-resolution
 * The editor MUST resolve selection by exact `internalSceneId` from the projected scene configs. A
 * null, stale, or missing selected ID MUST render the no-selection state and expose no editor
 * mutations.
 */
/**
 * @cc [owner:mixxorz,label:product] selected-scene-link-and-cue-state
 * Link/delete controls MUST appear only when the resolved scene has no LV1 index. The scene is cued
 * only when the active cue list's projected cued entry references that scene's exact internal ID;
 * duration and stored-channel scope editing MUST remain available for an unlinked scene.
 */
export function SceneEditor() {
  const { appState } = useAppState();
  const selected = appState.sceneConfigs.find(
    (scene) => scene.internalSceneId === appState.selectedSceneInternalId,
  );
  const activeCueList = appState.cueLists.find(
    (list) => list.id === appState.activeCueListId,
  );
  const cuedEntry = activeCueList?.entries.find(
    (entry) => entry.id === appState.cuedCueEntryId,
  );

  if (!selected) {
    return <EmptySceneSelection />;
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <SelectedSceneHeader
        currentScene={appState.currentScene}
        cued={cuedEntry?.sceneInternalId === selected.internalSceneId}
        scene={selected}
      />
      {selected.sceneIndex === null ? (
        <LinkSceneControls
          existingConfigs={appState.sceneConfigs}
          lv1Scenes={appState.scenes}
          scene={selected}
        />
      ) : null}
      <ChannelScopeGrid scene={selected} />
    </div>
  );
}

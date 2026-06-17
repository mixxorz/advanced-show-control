import { useAppCommands, useAppState } from "../appHooks";
import { ChannelScopeGrid } from "./ChannelScopeGrid";
import { SceneList } from "./SceneList";
import { SelectedSceneHeader } from "./SelectedSceneHeader";

export function SceneTab() {
  const { appState } = useAppState();
  const commands = useAppCommands();
  const selected = appState.sceneConfigs.find(
    (scene) => scene.sceneId === appState.selectedSceneId,
  );

  return (
    <div className="grid h-full min-h-0 gap-3 lg:grid-cols-[23rem_1fr]">
      <SceneList
        currentScene={appState.currentScene}
        onSelectScene={commands.selectScene}
        scenes={appState.sceneConfigs}
        selectedSceneId={appState.selectedSceneId}
      />
      <div className="min-h-0 space-y-3 overflow-auto">
        {selected ? (
          <>
            <SelectedSceneHeader
              onStore={() => commands.storeSceneConfig(selected.sceneId)}
              onToggleFaders={() =>
                commands.setSceneScopeFadersEnabled(
                  selected.sceneId,
                  !selected.scopeToggles.faders,
                )
              }
              onTogglePan={() =>
                commands.setSceneScopePanEnabled(
                  selected.sceneId,
                  !selected.scopeToggles.pan,
                )
              }
              scene={selected}
              setSceneDurationMs={commands.setSceneDurationMs}
            />
            <ChannelScopeGrid
              channels={appState.channels}
              scene={selected}
              setAllChannelsScoped={commands.setAllChannelsScoped}
              setChannelScoped={commands.setChannelScoped}
            />
          </>
        ) : (
          <div className="rounded-console-panel border border-console-line bg-console-panel p-4 text-console-muted">
            Select a scene to edit its scoped channels.
          </div>
        )}
      </div>
    </div>
  );
}

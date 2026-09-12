import type { SceneConfig } from "../types";
import { useAppCommands, useAppState } from "../appHooks";
import { ConsoleButton } from "./ConsoleButton";

/**
 * @cc [owner:mixxorz,label:product;safety] selected-scene-action-gating
 * Store MUST be disabled for an unlinked scene. Copy MUST remain available for linked and unlinked
 * scenes. Paste MUST be disabled unless the projected clipboard is available and the destination
 * scene is linked; each enabled action MUST dispatch only the supplied scene's internal ID.
 */
export function SelectedSceneActions(props: { scene: SceneConfig }) {
  const commands = useAppCommands();
  const {
    appState: { sceneSettingsClipboardAvailable },
  } = useAppState();
  const unlinked = props.scene.sceneIndex === null;

  return (
    <div className="flex items-center gap-2">
      <ConsoleButton
        disabled={unlinked}
        onClick={() => commands.storeSceneConfig(props.scene.internalSceneId)}
        variant="secondary"
      >
        Store
      </ConsoleButton>
      <ConsoleButton
        onClick={() => commands.copySceneSettings(props.scene.internalSceneId)}
        variant="secondary"
      >
        Copy
      </ConsoleButton>
      <ConsoleButton
        disabled={!sceneSettingsClipboardAvailable || unlinked}
        onClick={() => commands.pasteSceneSettings(props.scene.internalSceneId)}
        variant="secondary"
      >
        Paste
      </ConsoleButton>
    </div>
  );
}

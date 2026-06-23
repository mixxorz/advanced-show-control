import type { SceneConfig } from "../types";
import { useAppCommands } from "../appHooks";
import { ConsoleButton } from "./ConsoleButton";

export function SelectedSceneActions(props: { scene: SceneConfig }) {
  const commands = useAppCommands();
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
      <ConsoleButton variant="secondary">Copy</ConsoleButton>
      <ConsoleButton disabled variant="secondary">
        Paste
      </ConsoleButton>
    </div>
  );
}

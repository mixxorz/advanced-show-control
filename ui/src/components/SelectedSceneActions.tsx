import { useAppCommands } from "../appHooks";
import { ConsoleButton } from "./ConsoleButton";

export function SelectedSceneActions(props: { sceneId: string }) {
  const commands = useAppCommands();

  return (
    <div className="flex items-center gap-2">
      <ConsoleButton
        onClick={() => commands.storeSceneConfig(props.sceneId)}
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

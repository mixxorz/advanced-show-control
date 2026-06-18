import { useAppCommands, useAppState } from "../appHooks";
import { Panel } from "./Panel";
import { ShowFileControls } from "./ShowFileControls";

export function SessionsTab() {
  const { appState } = useAppState();
  const commands = useAppCommands();

  return (
    <Panel className="h-full min-h-0 p-4">
      <div className="grid max-w-xl gap-4">
        <h2 className="text-lg font-normal uppercase text-console-primary">
          Sessions
        </h2>
        <ShowFileControls
          dirty={appState.showFileDirty}
          fileName={appState.showFileName}
          filePath={appState.showFilePath}
          onNew={commands.newShowFile}
          onOpen={commands.openShowFile}
          onSave={commands.saveShowFile}
          onSaveAs={commands.saveShowFileAs}
        />
      </div>
    </Panel>
  );
}

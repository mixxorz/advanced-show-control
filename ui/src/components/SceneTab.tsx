import { SceneEditor } from "./SceneEditor";
import { SceneList } from "./SceneList";

export function SceneTab() {
  return (
    <div className="grid h-full min-h-0 gap-3 lg:grid-cols-[23rem_1fr]">
      <SceneList />
      <div className="min-h-0 overflow-hidden">
        <SceneEditor />
      </div>
    </div>
  );
}

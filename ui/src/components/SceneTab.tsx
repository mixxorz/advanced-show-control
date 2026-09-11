import { SceneEditor } from "./SceneEditor";
import { SceneList } from "./SceneList";

/**
 * @cc [owner:mixxorz,label:architecture] scene-tab-shared-projection
 * The scene tab MUST compose the production scene list and editor under the same app-state and
 * command providers so selection requests and projected editor selection cannot diverge through
 * component-local ownership.
 */
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

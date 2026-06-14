use advanced_show_control::runtime::events::AppEvent;

use super::shell::ShellState;
use super::view::AppViewState;

impl ShellState {
    pub async fn project_event_for_generation(
        &self,
        generation: u64,
        event: &AppEvent,
    ) -> Option<AppViewState> {
        match event {
            AppEvent::SceneRecall(scene_recall_event) => {
                self.apply_scene_recall_event_for_generation(generation, scene_recall_event)
                    .await
            }
            _ => None,
        }
    }
}

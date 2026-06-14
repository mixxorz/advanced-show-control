use advanced_show_control::runtime::events::AppEvent;

use super::shell::ShellState;
use super::view::{AppViewState, LogSeverity};

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
            AppEvent::Diagnostic { source, message } => {
                let log_message = format!("{source}: {message}");
                self.append_log(LogSeverity::Warning, log_message).await;
                Some(self.snapshot().await)
            }
            _ => None,
        }
    }
}

use advanced_show_control::runtime::events::AppEvent;

use super::shell::ShellState;
use super::view::{LogSeverity, LogSource};

impl ShellState {
    pub async fn apply_projector_event_to_projection(
        &self,
        generation: u64,
        event: &AppEvent,
    ) -> bool {
        match event {
            AppEvent::SceneRecall(scene_recall_event) => {
                self.apply_scene_recall_event_to_projection(generation, scene_recall_event)
                    .await
            }
            AppEvent::Diagnostic { source, message } => {
                let log_message = format!("{source}: {message}");
                self.push_log_for_generation(
                    generation,
                    LogSource::App,
                    LogSeverity::Warning,
                    log_message,
                )
                .await
            }
            _ => false,
        }
    }
}

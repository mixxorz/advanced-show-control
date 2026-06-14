use advanced_show_control::runtime::events::AppEvent;

use super::shell::ShellState;
use super::view::{AppViewState, LogSeverity, LogSource};

impl ShellState {
    pub async fn project_event_without_snapshot_for_generation(
        &self,
        generation: u64,
        event: &AppEvent,
    ) -> bool {
        match event {
            AppEvent::SceneRecall(scene_recall_event) => {
                self.apply_scene_recall_event_without_snapshot_for_generation(
                    generation,
                    scene_recall_event,
                )
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

    #[allow(dead_code)]
    pub async fn project_event_for_generation(
        &self,
        generation: u64,
        event: &AppEvent,
    ) -> Option<AppViewState> {
        if !self
            .project_event_without_snapshot_for_generation(generation, event)
            .await
        {
            return None;
        }

        self.snapshot_for_generation(generation).await
    }
}

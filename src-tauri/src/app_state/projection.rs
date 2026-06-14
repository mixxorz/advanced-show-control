use advanced_show_control::runtime::events::AppEvent;

use super::shell::ShellState;
use super::view::{LogSeverity, LogSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionOutcome {
    Applied,
    Stale,
    Ignored,
}

impl ProjectionOutcome {
    pub fn was_applied(self) -> bool {
        matches!(self, Self::Applied)
    }
}

impl ShellState {
    pub async fn apply_projector_event_to_projection(
        &self,
        generation: u64,
        event: &AppEvent,
    ) -> ProjectionOutcome {
        match event {
            AppEvent::SceneRecall(scene_recall_event) => {
                self.apply_scene_recall_event_to_projection(generation, scene_recall_event)
                    .await
            }
            AppEvent::Diagnostic { source, message } => {
                let log_message = format!("{source}: {message}");
                if self
                    .push_log_for_generation(
                        generation,
                        LogSource::App,
                        LogSeverity::Warning,
                        log_message,
                    )
                    .await
                {
                    ProjectionOutcome::Applied
                } else {
                    ProjectionOutcome::Stale
                }
            }
            _ => ProjectionOutcome::Ignored,
        }
    }
}

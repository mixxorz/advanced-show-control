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
            AppEvent::Show(_) => self.snapshot_for_generation(generation).await,
            AppEvent::SceneRecall(scene_recall_event) => {
                self.apply_scene_recall_event_for_generation(generation, scene_recall_event)
                    .await
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::AppConnectionState;
    use advanced_show_control::runtime::events::AppEvent;
    use advanced_show_control::show::events::ShowEvent;

    #[tokio::test]
    async fn show_event_projects_fresh_snapshot() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;

        let snapshot = state
            .project_event_for_generation(generation, &AppEvent::Show(ShowEvent::StateChanged))
            .await
            .expect("show event should project to current snapshot");

        assert_eq!(snapshot.connection, AppConnectionState::Connecting);
    }
}

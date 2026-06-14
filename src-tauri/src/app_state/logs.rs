use advanced_show_control::scene_recall::events::SceneRecallEvent;

use super::shell::ShellState;
use super::view::AppViewState;

impl ShellState {
    pub async fn apply_scene_recall_event_for_generation(
        &self,
        generation: u64,
        event: &SceneRecallEvent,
    ) -> Option<AppViewState> {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return None;
        }

        apply_scene_recall_event_locked(&mut inner, event);
        drop(inner);
        Some(self.snapshot().await)
    }
}

fn apply_scene_recall_event_locked(
    _inner: &mut super::shell::ShellInner,
    event: &SceneRecallEvent,
) {
    match event {
        SceneRecallEvent::Blocked { .. }
        | SceneRecallEvent::Skipped { .. }
        | SceneRecallEvent::Ready { .. }
        | SceneRecallEvent::StartRequested { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use advanced_show_control::scene_recall::events::SceneRecallEvent;

    #[tokio::test]
    async fn scene_recall_blocked_event_does_not_append_ui_log() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;

        let snapshot = state
            .apply_scene_recall_event_for_generation(
                generation,
                &SceneRecallEvent::Blocked {
                    scene_label: "1: Intro".to_string(),
                    reason: "locked out".to_string(),
                },
            )
            .await
            .expect("event should apply to current generation");

        assert!(
            snapshot
                .logs
                .iter()
                .all(|entry| entry.message != "Scene recall blocked for 1: Intro: locked out")
        );
    }
}

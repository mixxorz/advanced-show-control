use advanced_show_control::scene_recall::events::SceneRecallEvent;

use super::shell::ShellState;
use super::view::{AppViewState, LogSeverity, LogSource};

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

fn apply_scene_recall_event_locked(inner: &mut super::shell::ShellInner, event: &SceneRecallEvent) {
    match event {
        SceneRecallEvent::Blocked {
            scene_label,
            reason,
        } => inner.push_log(
            LogSource::App,
            LogSeverity::Warning,
            format!("Scene recall blocked for {scene_label}: {reason}"),
        ),
        SceneRecallEvent::Skipped {
            scene_label,
            reason,
        } => inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            format!("Scene recall skipped for {scene_label}: {reason}"),
        ),
        SceneRecallEvent::Ready {
            scene_label,
            target_count,
        } => inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            format!("Scene recall ready for {scene_label} ({target_count} targets)"),
        ),
        SceneRecallEvent::StartRequested { scene_label } => inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            format!("Scene recall start requested for {scene_label}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use advanced_show_control::scene_recall::events::SceneRecallEvent;

    #[tokio::test]
    async fn scene_recall_blocked_event_is_logged() {
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

        assert_eq!(
            snapshot.logs.last().unwrap().message,
            "Scene recall blocked for 1: Intro: locked out"
        );
    }
}

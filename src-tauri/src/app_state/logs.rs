use advanced_show_control::scene_recall::events::SceneRecallEvent;

use super::shell::ShellState;
use super::view::{LogSeverity, LogSource};

impl ShellState {
    pub async fn apply_scene_recall_event_to_projection(
        &self,
        generation: u64,
        event: &SceneRecallEvent,
    ) -> bool {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return false;
        }

        apply_scene_recall_event_locked(&mut inner, event);
        true
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

        assert!(
            state
                .apply_scene_recall_event_to_projection(
                    generation,
                    &SceneRecallEvent::Blocked {
                        scene_label: "1: Intro".to_string(),
                        reason: "locked out".to_string(),
                    },
                )
                .await
        );

        let snapshot = state
            .snapshot_for_generation(generation)
            .await
            .expect("event should apply to current generation");

        assert_eq!(
            snapshot.logs.last().unwrap().message,
            "Scene recall blocked for 1: Intro: locked out"
        );
    }
}

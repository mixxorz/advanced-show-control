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
    use crate::app_state::view::{ChannelConfig, ChannelRef, SceneConfig, ShowSnapshot};
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

    #[tokio::test]
    async fn show_event_projects_pan_family_fields() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state
            .show
            .replace_snapshot(ShowSnapshot {
                lockout: false,
                scene_configs: vec![SceneConfig {
                    scene_id: "1::Intro".to_string(),
                    scene_index: 1,
                    scene_name: "Intro".to_string(),
                    duration_ms: 5000,
                    channel_configs: vec![ChannelConfig {
                        group: 0,
                        channel: 2,
                        fader_db: Some(-8.0),
                        pan: Some(-12.0),
                        balance: Some(3.0),
                        width: Some(1.2),
                        pan_mode: Some(advanced_show_control::lv1::types::PanMode::Stereo),
                    }],
                    scoped_channels: vec![ChannelRef {
                        group: 0,
                        channel: 2,
                    }],
                    scope_toggles: advanced_show_control::show::types::SceneScopeToggles {
                        faders: true,
                        pan: true,
                    },
                }],
            })
            .await;

        let snapshot = state
            .project_event_for_generation(generation, &AppEvent::Show(ShowEvent::StateChanged))
            .await
            .expect("show event should project to current snapshot");

        assert!(snapshot.scene_configs[0].scope_toggles.pan);
        assert_eq!(
            snapshot.scene_configs[0].channel_configs[0].pan,
            Some(-12.0)
        );
        assert_eq!(
            snapshot.scene_configs[0].channel_configs[0].balance,
            Some(3.0)
        );
        assert_eq!(
            snapshot.scene_configs[0].channel_configs[0].width,
            Some(1.2)
        );
        assert_eq!(
            snapshot.scene_configs[0].channel_configs[0].pan_mode,
            Some(advanced_show_control::lv1::types::PanMode::Stereo)
        );
    }
}

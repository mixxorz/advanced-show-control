use crate::lv1::types::SceneListEntry;

use super::types::{SceneConfig, ShowSnapshot};

#[derive(Debug, Clone, PartialEq)]
pub struct ShowState {
    pub lockout: bool,
    pub scene_configs: Vec<SceneConfig>,
}

impl ShowState {
    pub fn snapshot(&self) -> ShowSnapshot {
        ShowSnapshot { lockout: self.lockout, scene_configs: self.scene_configs.clone() }
    }

    pub fn reconcile_scene_fade_configs(&mut self, scenes: &[SceneListEntry]) {
        self.scene_configs.retain(|scene| scenes.iter().any(|entry| entry.name == scene.scene_id));
    }

    pub fn get_scene_config(&self, scene_id: &str) -> Option<SceneConfig> {
        self.scene_configs.iter().find(|scene| scene.scene_id == scene_id).cloned()
    }

    pub fn set_lockout(&mut self, enabled: bool) -> bool {
        if self.lockout == enabled { false } else { self.lockout = enabled; true }
    }

    pub(crate) fn get_scene_config_mut(&mut self, scene_id: &str) -> Option<&mut SceneConfig> {
        self.scene_configs.iter_mut().find(|scene| scene.scene_id == scene_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::types::ChannelInfo;
    use crate::show::{ChannelConfig, ChannelRef};

    fn scene_config(scene_id: &str, duration_ms: u64, channels: Vec<ChannelConfig>) -> SceneConfig {
        SceneConfig {
            scene_id: scene_id.to_string(),
            duration_ms,
            channels,
        }
    }

    #[test]
    fn reconciliation_preserves_matching_config_data() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                "scene-1",
                1_500,
                vec![ChannelConfig {
                    channel: ChannelRef { group: 0, channel: 1 },
                    scoped: true,
                    target_db: -12.0,
                }],
            )],
        };

        state.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 1,
            name: "scene-1".to_string(),
        }]);

        assert_eq!(state.scene_configs[0].duration_ms, 1_500);
        assert!(state.scene_configs[0].channels[0].scoped);
        assert_eq!(state.scene_configs[0].channels[0].target_db, -12.0);
    }

    #[test]
    fn invalid_duration_rejected() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config("scene-1", 1_000, vec![])],
        };

        let err = state.set_scene_duration_ms("scene-1", 99).unwrap_err();

        assert_eq!(err, "Fade duration must be between 100 ms and 120000 ms");
    }

    #[test]
    fn channel_scope_mutation_toggles_and_reports_noop() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                "scene-1",
                1_000,
                vec![ChannelConfig {
                    channel: ChannelRef { group: 0, channel: 1 },
                    scoped: false,
                    target_db: -9.0,
                }],
            )],
        };

        assert!(state
            .set_channel_scoped("scene-1", 0, 1, true)
            .unwrap());
        assert!(!state
            .set_channel_scoped("scene-1", 0, 1, true)
            .unwrap());
    }

    #[test]
    fn store_scene_config_snapshots_current_channels_and_scopes_first_store() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![],
        };
        let channels = vec![ChannelInfo {
            group: 0,
            channel: 1,
            name: "Ch 1".to_string(),
            gain_db: -7.5,
            muted: false,
        }];

        assert!(state.store_scene_config("scene-1", &channels).unwrap());

        let snapshot = state.snapshot();
        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(snapshot.scene_configs[0].channels[0].channel.group, 0);
        assert_eq!(snapshot.scene_configs[0].channels[0].channel.channel, 1);
        assert!(snapshot.scene_configs[0].channels[0].scoped);
        assert_eq!(snapshot.scene_configs[0].channels[0].target_db, -7.5);
    }
}

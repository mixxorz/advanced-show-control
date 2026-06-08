use crate::lv1::types::SceneListEntry;

use super::types::scene_id;
use super::types::{SceneConfig, ShowSnapshot};

#[derive(Debug, Clone, PartialEq)]
pub struct ShowState {
    pub lockout: bool,
    pub scene_configs: Vec<SceneConfig>,
}

impl ShowState {
    pub fn snapshot(&self) -> ShowSnapshot {
        ShowSnapshot {
            lockout: self.lockout,
            scene_configs: self.scene_configs.clone(),
        }
    }

    pub fn reconcile_scene_fade_configs(&mut self, scenes: &[SceneListEntry]) -> bool {
        let mut next = Vec::with_capacity(scenes.len());
        for entry in scenes {
            let id = scene_id(entry.index, &entry.name);
            if let Some(existing) = self.scene_configs.iter().find(|scene| scene.scene_id == id) {
                next.push(existing.clone());
            } else {
                next.push(SceneConfig {
                    scene_id: id,
                    scene_index: entry.index,
                    scene_name: entry.name.clone(),
                    duration_ms: 0,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                });
            }
        }

        let changed = next != self.scene_configs;
        self.scene_configs = next;
        changed
    }

    pub fn replace_snapshot(&mut self, snapshot: ShowSnapshot) {
        self.lockout = snapshot.lockout;
        self.scene_configs = snapshot.scene_configs;
    }

    pub fn clear(&mut self) {
        self.lockout = false;
        self.scene_configs.clear();
    }

    pub fn get_scene_config(&self, scene_id: &str) -> Option<SceneConfig> {
        self.scene_configs
            .iter()
            .find(|scene| scene.scene_id == scene_id)
            .cloned()
    }

    pub fn set_lockout(&mut self, enabled: bool) -> bool {
        if self.lockout == enabled {
            false
        } else {
            self.lockout = enabled;
            true
        }
    }

    pub(crate) fn get_scene_config_mut(&mut self, scene_id: &str) -> Option<&mut SceneConfig> {
        self.scene_configs
            .iter_mut()
            .find(|scene| scene.scene_id == scene_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::types::ChannelInfo;
    use crate::show::ChannelConfig;

    fn scene_config(scene_id: &str, duration_ms: u64, channels: Vec<ChannelConfig>) -> SceneConfig {
        let (scene_index, scene_name) = scene_id
            .split_once("::")
            .map(|(index, name)| (index.parse().unwrap(), name.to_string()))
            .unwrap();
        SceneConfig {
            scene_id: scene_id.to_string(),
            scene_index,
            scene_name,
            duration_ms,
            channel_configs: channels,
            scoped_channels: vec![],
        }
    }

    #[test]
    fn reconciliation_preserves_matching_config_data() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                "1::scene-1",
                1_500,
                vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-12.0),
                }],
            )],
        };

        assert!(!state.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 1,
            name: "scene-1".to_string(),
        }]));

        assert_eq!(state.scene_configs[0].duration_ms, 1_500);
        assert_eq!(state.scene_configs[0].scene_index, 1);
        assert_eq!(state.scene_configs[0].scene_name, "scene-1");
        assert_eq!(
            state.scene_configs[0].channel_configs[0].fader_db,
            Some(-12.0)
        );
    }

    #[test]
    fn reconciliation_reports_noop_when_scene_list_matches_current_configs() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                "1::scene-1",
                1_500,
                vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-12.0),
                }],
            )],
        };

        assert!(!state.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 1,
            name: "scene-1".to_string(),
        }]));
    }

    #[test]
    fn reconciliation_replaces_same_name_different_index_scene_with_default_config() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                "1::scene-1",
                1_500,
                vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-12.0),
                }],
            )],
        };

        assert!(state.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 2,
            name: "scene-1".to_string(),
        }]));
        assert_eq!(state.scene_configs.len(), 1);
        assert_eq!(state.scene_configs[0].scene_id, "2::scene-1");
        assert_eq!(state.scene_configs[0].scene_index, 2);
        assert_eq!(state.scene_configs[0].scene_name, "scene-1");
        assert!(state.scene_configs[0].channel_configs.is_empty());
        assert!(state.scene_configs[0].scoped_channels.is_empty());
    }

    #[test]
    fn invalid_duration_rejected() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config("1::scene-1", 1_000, vec![])],
        };

        let err = state.set_scene_duration_ms("1::scene-1", 99).unwrap_err();

        assert_eq!(err, "Fade duration must be between 100 ms and 120000 ms");
    }

    #[test]
    fn channel_scope_mutation_toggles_and_reports_noop() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                "1::scene-1",
                1_000,
                vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-9.0),
                }],
            )],
        };

        assert!(state.set_channel_scoped("1::scene-1", 0, 1, true).unwrap());
        assert!(!state.set_channel_scoped("1::scene-1", 0, 1, true).unwrap());
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

        assert!(state.store_scene_config("1::scene-1", &channels).unwrap());

        let snapshot = state.snapshot();
        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(snapshot.scene_configs[0].scene_index, 1);
        assert_eq!(snapshot.scene_configs[0].scene_name, "scene-1");
        assert_eq!(snapshot.scene_configs[0].channel_configs[0].group, 0);
        assert_eq!(snapshot.scene_configs[0].channel_configs[0].channel, 1);
        assert_eq!(
            snapshot.scene_configs[0].channel_configs[0].fader_db,
            Some(-7.5)
        );
        assert_eq!(snapshot.scene_configs[0].scoped_channels[0].group, 0);
        assert_eq!(snapshot.scene_configs[0].scoped_channels[0].channel, 1);
    }
}

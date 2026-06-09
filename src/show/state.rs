use crate::lv1::types::SceneListEntry;

use super::types::scene_id;
use super::types::{SceneConfig, SceneScopeToggles, ShowSnapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
struct SceneEntry {
    index: i32,
    name: String,
}

fn entries_from_configs(configs: &[SceneConfig]) -> Vec<SceneEntry> {
    let mut entries: Vec<_> = configs
        .iter()
        .map(|scene| SceneEntry {
            index: scene.scene_index,
            name: scene.scene_name.clone(),
        })
        .collect();
    entries.sort_by_key(|entry| entry.index);
    entries
}

fn entries_from_scene_list(scenes: &[SceneListEntry]) -> Vec<SceneEntry> {
    let mut entries: Vec<_> = scenes
        .iter()
        .map(|scene| SceneEntry {
            index: scene.index,
            name: scene.name.clone(),
        })
        .collect();
    entries.sort_by_key(|entry| entry.index);
    entries
}

fn default_scene_config(entry: &SceneEntry) -> SceneConfig {
    SceneConfig {
        scene_id: scene_id(entry.index, &entry.name),
        scene_index: entry.index,
        scene_name: entry.name.clone(),
        duration_ms: 0,
        channel_configs: Vec::new(),
        scoped_channels: Vec::new(),
        scope_toggles: SceneScopeToggles::default(),
    }
}

fn update_scene_locator(config: &mut SceneConfig, entry: &SceneEntry) {
    config.scene_id = scene_id(entry.index, &entry.name);
    config.scene_index = entry.index;
    config.scene_name = entry.name.clone();
}

#[derive(Debug, Clone, PartialEq, Default)]
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
        let old_entries = entries_from_configs(&self.scene_configs);
        let new_entries = entries_from_scene_list(scenes);

        if old_entries == new_entries {
            return false;
        }

        if old_entries.len() == new_entries.len() {
            let changed_at: Vec<_> = old_entries
                .iter()
                .zip(new_entries.iter())
                .enumerate()
                .filter(|(_, (old, new))| old != new)
                .collect();
            if changed_at.len() == 1 {
                let (_, (old, new)) = changed_at[0];
                if old.index == new.index
                    && let Some(scene) = self.scene_configs.iter_mut().find(|scene| {
                        scene.scene_index == old.index && scene.scene_name == old.name
                    })
                {
                    update_scene_locator(scene, new);
                    self.scene_configs.sort_by_key(|scene| scene.scene_index);
                    return true;
                }
            }
        }

        let mut next = Vec::with_capacity(scenes.len());
        for entry in &new_entries {
            let id = scene_id(entry.index, &entry.name);
            if let Some(existing) = self.scene_configs.iter().find(|scene| scene.scene_id == id) {
                next.push(existing.clone());
            } else {
                next.push(default_scene_config(entry));
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

    fn channel(group: i32, channel: i32, name: &str, gain_db: f64) -> ChannelInfo {
        ChannelInfo {
            group,
            channel,
            name: name.to_string(),
            gain_db,
            muted: false,
        }
    }

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
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    #[test]
    fn reconciliation_tracks_single_scene_rename() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                "1::Verse",
                1_500,
                vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-12.0),
                }],
            )],
        };

        assert!(state.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 1,
            name: "Verse Big".to_string(),
        }]));

        assert_eq!(state.scene_configs.len(), 1);
        assert_eq!(state.scene_configs[0].scene_id, "1::Verse Big");
        assert_eq!(state.scene_configs[0].scene_index, 1);
        assert_eq!(state.scene_configs[0].scene_name, "Verse Big");
        assert_eq!(state.scene_configs[0].duration_ms, 1_500);
        assert_eq!(
            state.scene_configs[0].channel_configs[0].fader_db,
            Some(-12.0)
        );
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

        assert_eq!(
            err,
            "Fade duration must be 0 or between 100 ms and 120000 ms"
        );
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

    #[test]
    fn store_scene_config_defaults_fader_scope_enabled() {
        let mut state = ShowState::default();
        let changed = state
            .store_scene_config("1::scene-1", &[channel(0, 1, "Lead", -6.0)])
            .unwrap();

        assert!(changed);
        assert!(state.scene_configs[0].scope_toggles.faders);
    }

    #[test]
    fn store_scene_config_preserves_fader_scope_toggle() {
        let mut state = ShowState::default();
        state
            .store_scene_config("1::scene-1", &[channel(0, 1, "Lead", -6.0)])
            .unwrap();
        assert!(
            state
                .set_scene_scope_faders_enabled("1::scene-1", false)
                .unwrap()
        );

        state
            .store_scene_config("1::scene-1", &[channel(0, 1, "Lead", -3.0)])
            .unwrap();

        assert!(!state.scene_configs[0].scope_toggles.faders);
    }

    #[test]
    fn scene_scope_fader_toggle_mutation_reports_noop() {
        let mut state = ShowState::default();
        state
            .store_scene_config("1::scene-1", &[channel(0, 1, "Lead", -6.0)])
            .unwrap();

        assert!(
            state
                .set_scene_scope_faders_enabled("1::scene-1", false)
                .unwrap()
        );
        assert!(
            !state
                .set_scene_scope_faders_enabled("1::scene-1", false)
                .unwrap()
        );
        assert!(!state.scene_configs[0].scope_toggles.faders);
    }

    #[test]
    fn scene_scope_fader_toggle_requires_existing_scene_config() {
        let mut state = ShowState::default();

        let err = state
            .set_scene_scope_faders_enabled("missing", false)
            .unwrap_err();

        assert_eq!(err, "Scene config not found");
    }

    #[test]
    fn scene_duration_allows_zero_for_immediate_movement() {
        let mut state = ShowState::default();
        state
            .store_scene_config("1::scene-1", &[channel(0, 1, "Lead", -6.0)])
            .unwrap();

        assert!(state.set_scene_duration_ms("1::scene-1", 0).unwrap());
        assert_eq!(state.scene_configs[0].duration_ms, 0);
    }
}

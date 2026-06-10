use std::collections::{HashMap, VecDeque};

use crate::lv1::types::SceneListEntry;

use super::types::scene_id;
use super::types::{SceneConfig, SceneScopeToggles, ShowSnapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
struct SceneEntry {
    index: i32,
    name: String,
}

fn entries_from_configs(configs: &[SceneConfig]) -> Vec<SceneEntry> {
    configs
        .iter()
        .map(|scene| SceneEntry {
            index: scene.scene_index,
            name: scene.scene_name.clone(),
        })
        .collect()
}

fn entries_from_scene_list(scenes: &[SceneListEntry]) -> Vec<SceneEntry> {
    scenes
        .iter()
        .map(|scene| SceneEntry {
            index: scene.index,
            name: scene.name.clone(),
        })
        .collect()
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum SceneListChange {
    Noop,
    Rename,
    Move { from: usize, to: usize },
    Insert { at: usize },
    Delete { at: usize },
    Ambiguous,
}

impl SceneListChange {
    fn diagnostic_label(&self) -> String {
        match self {
            Self::Noop => "noop".to_string(),
            Self::Rename => "rename".to_string(),
            Self::Move { from, to } => format!("move from={from} to={to}"),
            Self::Insert { at } => format!("insert at={at}"),
            Self::Delete { at } => format!("delete at={at}"),
            Self::Ambiguous => "ambiguous exact-match-fallback".to_string(),
        }
    }
}

fn without_at(entries: &[SceneEntry], at: usize) -> Vec<SceneEntry> {
    entries
        .iter()
        .enumerate()
        .filter_map(|(idx, entry)| if idx == at { None } else { Some(entry.clone()) })
        .collect()
}

fn names(entries: &[SceneEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

fn describe_entries(entries: &[SceneEntry]) -> String {
    entries
        .iter()
        .map(|entry| format!("{}:{:?}", entry.index, entry.name))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn move_candidates(old: &[SceneEntry], new: &[SceneEntry]) -> Vec<(usize, usize)> {
    if old.len() != new.len() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for from in 0..old.len() {
        let remaining = without_at(old, from);
        let moved = old[from].clone();
        for to in 0..old.len() {
            let mut candidate = remaining.clone();
            candidate.insert(to, moved.clone());
            if names(&candidate) == names(new) {
                matches.push((from, to));
            }
        }
    }
    matches.sort_unstable();
    matches.dedup();
    matches
}

fn name_counts(entries: &[SceneEntry]) -> Vec<String> {
    let mut counts = HashMap::<String, usize>::new();
    for entry in entries {
        *counts.entry(entry.name.clone()).or_default() += 1;
    }
    let mut duplicates = counts
        .into_iter()
        .filter_map(|(name, count)| (count > 1).then(|| format!("{name}x{count}")))
        .collect::<Vec<_>>();
    duplicates.sort();
    duplicates
}

fn classify_scene_list_change(old: &[SceneEntry], new: &[SceneEntry]) -> SceneListChange {
    if old == new {
        return SceneListChange::Noop;
    }

    if old.len() == new.len() {
        let changed_indexes: Vec<_> = old
            .iter()
            .zip(new.iter())
            .enumerate()
            .filter_map(|(idx, (old_entry, new_entry))| {
                if old_entry == new_entry {
                    None
                } else {
                    Some(idx)
                }
            })
            .collect();

        if changed_indexes.len() == 1 {
            let idx = changed_indexes[0];
            if old[idx].index == new[idx].index {
                return SceneListChange::Rename;
            }
        }

        let matches = move_candidates(old, new);
        return match matches.as_slice() {
            [(from, to)] if from != to => SceneListChange::Move {
                from: *from,
                to: *to,
            },
            _ => SceneListChange::Ambiguous,
        };
    }

    if new.len() == old.len() + 1 {
        let matches: Vec<_> = (0..new.len())
            .filter(|at| names(&without_at(new, *at)) == names(old))
            .collect();
        return match matches.as_slice() {
            [at] => SceneListChange::Insert { at: *at },
            _ => SceneListChange::Ambiguous,
        };
    }

    if old.len() == new.len() + 1 {
        let matches: Vec<_> = (0..old.len())
            .filter(|at| names(&without_at(old, *at)) == names(new))
            .collect();
        return match matches.as_slice() {
            [at] => SceneListChange::Delete { at: *at },
            _ => SceneListChange::Ambiguous,
        };
    }

    SceneListChange::Ambiguous
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

        match classify_scene_list_change(&old_entries, &new_entries) {
            SceneListChange::Noop => false,
            SceneListChange::Rename => self.apply_position_mapping(&new_entries),
            SceneListChange::Move { .. }
            | SceneListChange::Insert { .. }
            | SceneListChange::Delete { .. }
            | SceneListChange::Ambiguous => {
                self.reconcile_scene_fade_configs_by_name_fifo(&new_entries)
            }
        }
    }

    pub fn scene_reconciliation_diagnostic(&self, scenes: &[SceneListEntry]) -> String {
        let old_entries = entries_from_configs(&self.scene_configs);
        let new_entries = entries_from_scene_list(scenes);
        let change = classify_scene_list_change(&old_entries, &new_entries);
        let candidates = move_candidates(&old_entries, &new_entries)
            .into_iter()
            .map(|(from, to)| format!("{from}->{to}"))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "scene reconciliation preview: change={} old=[{}] new=[{}] move_candidates=[{}] duplicate_names=[{}] strategy={}",
            change.diagnostic_label(),
            describe_entries(&old_entries),
            describe_entries(&new_entries),
            candidates,
            name_counts(&new_entries).join(","),
            if matches!(change, SceneListChange::Rename | SceneListChange::Noop) {
                "classified"
            } else {
                "name-keyed-fifo"
            },
        )
    }

    fn apply_position_mapping(&mut self, entries: &[SceneEntry]) -> bool {
        self.replace_scene_configs_with_entries(self.scene_configs.clone(), entries)
    }

    fn replace_scene_configs_with_entries(
        &mut self,
        mut configs: Vec<SceneConfig>,
        entries: &[SceneEntry],
    ) -> bool {
        for (config, entry) in configs.iter_mut().zip(entries.iter()) {
            update_scene_locator(config, entry);
        }

        let changed = configs != self.scene_configs;
        self.scene_configs = configs;
        changed
    }

    fn reconcile_scene_fade_configs_by_name_fifo(&mut self, entries: &[SceneEntry]) -> bool {
        let mut old_by_name = HashMap::<String, VecDeque<SceneConfig>>::new();
        let old_configs = std::mem::take(&mut self.scene_configs);
        for scene in old_configs.iter().cloned() {
            old_by_name
                .entry(scene.scene_name.clone())
                .or_default()
                .push_back(scene);
        }

        let mut next = Vec::with_capacity(entries.len());
        for entry in entries {
            let mut scene = old_by_name
                .get_mut(&entry.name)
                .and_then(VecDeque::pop_front)
                .unwrap_or_else(|| default_scene_config(entry));
            update_scene_locator(&mut scene, entry);
            next.push(scene);
        }

        let changed = next != old_configs;
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
            pan: None,
            balance: None,
            width: None,
            pan_mode: None,
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

    fn named_scene_config(index: i32, name: &str, duration_ms: u64) -> SceneConfig {
        scene_config(
            &format!("{index}::{name}"),
            duration_ms,
            vec![ChannelConfig {
                group: 0,
                channel: index,
                fader_db: Some(-10.0 - f64::from(index)),
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
        )
    }

    fn scene_entry(index: i32, name: &str) -> SceneListEntry {
        SceneListEntry {
            index,
            name: name.to_string(),
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
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
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
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
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
    fn reconciliation_preserves_incoming_scene_order() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                scene_config("2::Chorus", 300, vec![]),
                scene_config("0::Intro", 100, vec![]),
                scene_config("1::Verse", 200, vec![]),
            ],
        };

        assert!(state.reconcile_scene_fade_configs(&[
            SceneListEntry {
                index: 1,
                name: "Verse".to_string(),
            },
            SceneListEntry {
                index: 0,
                name: "Intro".to_string(),
            },
            SceneListEntry {
                index: 2,
                name: "Chorus".to_string(),
            },
        ]));

        assert_eq!(state.scene_configs[0].scene_id, "1::Verse");
        assert_eq!(state.scene_configs[1].scene_id, "0::Intro");
        assert_eq!(state.scene_configs[2].scene_id, "2::Chorus");
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
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
            )],
        };

        assert!(!state.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 1,
            name: "scene-1".to_string(),
        }]));
    }

    #[test]
    fn reconciliation_preserves_same_name_different_index_scene_config() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                "1::scene-1",
                1_500,
                vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-12.0),
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
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
        assert_eq!(state.scene_configs[0].duration_ms, 1_500);
        assert_eq!(
            state.scene_configs[0].channel_configs[0].fader_db,
            Some(-12.0)
        );
    }

    #[test]
    fn reconciliation_tracks_scene_move_later() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                named_scene_config(0, "Intro", 100),
                named_scene_config(1, "Verse", 200),
                named_scene_config(2, "Chorus", 300),
            ],
        };

        assert!(state.reconcile_scene_fade_configs(&[
            scene_entry(0, "Verse"),
            scene_entry(1, "Chorus"),
            scene_entry(2, "Intro"),
        ]));

        assert_eq!(state.scene_configs[0].scene_id, "0::Verse");
        assert_eq!(state.scene_configs[0].duration_ms, 200);
        assert_eq!(state.scene_configs[1].scene_id, "1::Chorus");
        assert_eq!(state.scene_configs[1].duration_ms, 300);
        assert_eq!(state.scene_configs[2].scene_id, "2::Intro");
        assert_eq!(state.scene_configs[2].duration_ms, 100);
    }

    #[test]
    fn reconciliation_tracks_scene_move_earlier() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                named_scene_config(0, "Intro", 100),
                named_scene_config(1, "Verse", 200),
                named_scene_config(2, "Chorus", 300),
            ],
        };

        assert!(state.reconcile_scene_fade_configs(&[
            scene_entry(0, "Chorus"),
            scene_entry(1, "Intro"),
            scene_entry(2, "Verse"),
        ]));

        assert_eq!(state.scene_configs[0].scene_id, "0::Chorus");
        assert_eq!(state.scene_configs[0].duration_ms, 300);
        assert_eq!(state.scene_configs[1].scene_id, "1::Intro");
        assert_eq!(state.scene_configs[1].duration_ms, 100);
        assert_eq!(state.scene_configs[2].scene_id, "2::Verse");
        assert_eq!(state.scene_configs[2].duration_ms, 200);
    }

    #[test]
    fn reconciliation_tracks_single_insert() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                named_scene_config(0, "Intro", 100),
                named_scene_config(1, "Chorus", 300),
            ],
        };

        assert!(state.reconcile_scene_fade_configs(&[
            scene_entry(0, "Intro"),
            scene_entry(1, "Verse"),
            scene_entry(2, "Chorus"),
        ]));

        assert_eq!(state.scene_configs[0].scene_id, "0::Intro");
        assert_eq!(state.scene_configs[0].duration_ms, 100);
        assert_eq!(state.scene_configs[1].scene_id, "1::Verse");
        assert_eq!(state.scene_configs[1].duration_ms, 0);
        assert!(state.scene_configs[1].channel_configs.is_empty());
        assert_eq!(state.scene_configs[2].scene_id, "2::Chorus");
        assert_eq!(state.scene_configs[2].duration_ms, 300);
    }

    #[test]
    fn reconciliation_tracks_single_delete() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                named_scene_config(0, "Intro", 100),
                named_scene_config(1, "Verse", 200),
                named_scene_config(2, "Chorus", 300),
            ],
        };

        assert!(
            state
                .reconcile_scene_fade_configs(&[scene_entry(0, "Intro"), scene_entry(1, "Chorus")])
        );

        assert_eq!(state.scene_configs.len(), 2);
        assert_eq!(state.scene_configs[0].scene_id, "0::Intro");
        assert_eq!(state.scene_configs[0].duration_ms, 100);
        assert_eq!(state.scene_configs[1].scene_id, "1::Chorus");
        assert_eq!(state.scene_configs[1].duration_ms, 300);
    }

    #[test]
    fn reconciliation_uses_exact_match_fallback_for_multi_operation_change() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                named_scene_config(0, "Intro", 100),
                named_scene_config(1, "Verse", 200),
            ],
        };

        assert!(state.reconcile_scene_fade_configs(&[
            scene_entry(0, "Intro New"),
            scene_entry(1, "Verse New")
        ]));

        assert_eq!(state.scene_configs[0].scene_id, "0::Intro New");
        assert_eq!(state.scene_configs[0].duration_ms, 0);
        assert_eq!(state.scene_configs[1].scene_id, "1::Verse New");
        assert_eq!(state.scene_configs[1].duration_ms, 0);
    }

    #[test]
    fn scene_reconciliation_diagnostic_describes_adjacent_move_ambiguity() {
        let state = ShowState {
            lockout: false,
            scene_configs: vec![
                named_scene_config(0, "Intro", 100),
                named_scene_config(1, "Verse", 200),
                named_scene_config(2, "Chorus", 300),
            ],
        };

        let diagnostic = state.scene_reconciliation_diagnostic(&[
            scene_entry(0, "Verse"),
            scene_entry(1, "Intro"),
            scene_entry(2, "Chorus"),
        ]);

        assert!(diagnostic.contains("change=ambiguous exact-match-fallback"));
        assert!(diagnostic.contains("old=[0:\"Intro\" | 1:\"Verse\" | 2:\"Chorus\"]"));
        assert!(diagnostic.contains("new=[0:\"Verse\" | 1:\"Intro\" | 2:\"Chorus\"]"));
        assert!(diagnostic.contains("move_candidates=[0->1,1->0]"));
    }

    #[test]
    fn reconciliation_tracks_adjacent_scene_move_by_name() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                named_scene_config(0, "Intro", 100),
                named_scene_config(1, "Verse", 200),
                named_scene_config(2, "Chorus", 300),
            ],
        };

        assert!(state.reconcile_scene_fade_configs(&[
            scene_entry(0, "Verse"),
            scene_entry(1, "Intro"),
            scene_entry(2, "Chorus"),
        ]));

        assert_eq!(state.scene_configs[0].scene_id, "0::Verse");
        assert_eq!(state.scene_configs[0].duration_ms, 200);
        assert_eq!(state.scene_configs[1].scene_id, "1::Intro");
        assert_eq!(state.scene_configs[1].duration_ms, 100);
        assert_eq!(state.scene_configs[2].scene_id, "2::Chorus");
        assert_eq!(state.scene_configs[2].duration_ms, 300);
    }

    #[test]
    fn reconciliation_uses_fifo_name_matching_for_duplicate_scene_names() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                named_scene_config(0, "Intro", 100),
                named_scene_config(1, "Dupe", 200),
                named_scene_config(2, "Dupe", 300),
                named_scene_config(3, "Chorus", 400),
            ],
        };

        assert!(state.reconcile_scene_fade_configs(&[
            scene_entry(0, "Dupe"),
            scene_entry(1, "Intro"),
            scene_entry(2, "Dupe"),
            scene_entry(3, "Chorus"),
        ]));

        assert_eq!(state.scene_configs[0].scene_id, "0::Dupe");
        assert_eq!(state.scene_configs[0].duration_ms, 200);
        assert_eq!(state.scene_configs[1].scene_id, "1::Intro");
        assert_eq!(state.scene_configs[1].duration_ms, 100);
        assert_eq!(state.scene_configs[2].scene_id, "2::Dupe");
        assert_eq!(state.scene_configs[2].duration_ms, 300);
        assert_eq!(state.scene_configs[3].scene_id, "3::Chorus");
        assert_eq!(state.scene_configs[3].duration_ms, 400);
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
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
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
            pan: None,
            balance: None,
            width: None,
            pan_mode: None,
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

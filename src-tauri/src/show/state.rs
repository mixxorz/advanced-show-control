use std::collections::{HashMap, VecDeque};

use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};
use crate::lv1::{Lv1StateSnapshot, SceneListEntry};
use crate::show::show_file::{ShowFile, export_show_file};

use super::types::{SceneConfig, SceneScopeToggles, ShowDocument};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SceneEntry {
    index: i32,
    name: String,
}

fn entries_from_configs(configs: &[SceneConfig]) -> Vec<SceneEntry> {
    configs
        .iter()
        .map(|scene| SceneEntry {
            index: scene
                .scene_index
                .expect("scene_index missing in reconciled config"),
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
        internal_scene_id: Uuid::new_v4(),
        scene_index: Some(entry.index),
        scene_name: entry.name.clone(),
        duration_ms: 0,
        channel_configs: Vec::new(),
        scoped_channels: Vec::new(),
        scope_toggles: SceneScopeToggles::default(),
    }
}

fn update_scene_locator(config: &mut SceneConfig, entry: &SceneEntry) {
    config.scene_index = Some(entry.index);
    config.scene_name = entry.name.clone();
}

fn scene_internal_id(scene_id: &str) -> Uuid {
    let _ = scene_id;
    Uuid::new_v4()
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
    lockout: bool,
    scene_configs: Vec<SceneConfig>,
    cued_scene_internal_id: Option<Uuid>,
    selected_scene_id: Option<String>,
    show_file_path: Option<std::path::PathBuf>,
    show_file_dirty: bool,
    show_file_last_saved_at: Option<String>,
    discovered_lv1_systems: Vec<DiscoveredLv1System>,
    connected_lv1_identity: Option<Lv1SystemIdentity>,
    pending_lv1_identity: Option<Lv1SystemIdentity>,
    reconnect: ReconnectState,
    last_event_at: Option<String>,
}

impl ShowState {
    pub(crate) fn reset_for_new_show(&mut self, lv1: Option<&Lv1StateSnapshot>) -> Option<String> {
        self.clear();
        if let Some(lv1) = lv1
            && !lv1.scene_list.is_empty()
        {
            self.reconcile_scene_fade_configs(&lv1.scene_list);
        }
        self.selected_scene_id = self
            .scene_configs
            .first()
            .map(|scene| scene.internal_scene_id.to_string());
        self.show_file_path = None;
        self.show_file_dirty = false;
        self.show_file_last_saved_at = None;
        self.selected_scene_id.clone()
    }

    pub(crate) fn mark_saved(&mut self, path: std::path::PathBuf, saved_at: String) {
        self.show_file_path = Some(path);
        self.show_file_last_saved_at = Some(saved_at);
        self.show_file_dirty = false;
    }

    pub(crate) fn set_selected_scene_id(&mut self, selected_scene_id: Option<String>) -> bool {
        if self.selected_scene_id == selected_scene_id {
            return false;
        }
        self.selected_scene_id = selected_scene_id;
        true
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.show_file_dirty = true;
    }

    pub(crate) fn set_discovered_lv1_systems(&mut self, systems: Vec<DiscoveredLv1System>) -> bool {
        if self.discovered_lv1_systems == systems {
            false
        } else {
            self.discovered_lv1_systems = systems;
            true
        }
    }

    pub(crate) fn set_pending_lv1_identity(&mut self, identity: Option<Lv1SystemIdentity>) -> bool {
        if self.pending_lv1_identity == identity {
            false
        } else {
            self.pending_lv1_identity = identity;
            true
        }
    }

    pub(crate) fn establish_connected_lv1_identity(&mut self, identity: Lv1SystemIdentity) -> bool {
        let changed = self.connected_lv1_identity.as_ref() != Some(&identity)
            || self.pending_lv1_identity.is_some();
        if changed {
            self.connected_lv1_identity = Some(identity);
            self.pending_lv1_identity = None;
        }
        changed
    }

    pub(crate) fn clear_connected_lv1_identity(&mut self) -> bool {
        if self.connected_lv1_identity.is_none() {
            false
        } else {
            self.connected_lv1_identity = None;
            true
        }
    }

    pub(crate) fn set_reconnect_state(&mut self, reconnect: ReconnectState) -> bool {
        if self.reconnect == reconnect {
            false
        } else {
            self.reconnect = reconnect;
            true
        }
    }

    pub(crate) fn handle_runtime_disconnected(&mut self, _reason: String) -> bool {
        let mut changed = false;
        if self.connected_lv1_identity.take().is_some() {
            changed = true;
        }
        if self.pending_lv1_identity.take().is_some() {
            changed = true;
        }
        let next = ReconnectState {
            active: false,
            attempt: 0,
        };
        if self.reconnect != next {
            self.reconnect = next;
            changed = true;
        }
        let timestamp = crate::time::current_timestamp_millis();
        if self.last_event_at.as_ref() != Some(&timestamp) {
            self.last_event_at = Some(timestamp);
            changed = true;
        }
        changed
    }

    pub(crate) fn lockout(&self) -> bool {
        self.lockout
    }

    pub(crate) fn current_show_file_path(&self) -> Option<std::path::PathBuf> {
        self.show_file_path.clone()
    }

    pub(crate) fn export_show_file(&self, saved_at: String) -> ShowFile {
        export_show_file(self.snapshot(), saved_at)
    }

    pub(crate) fn scene_configs_mut(&mut self) -> &mut Vec<SceneConfig> {
        &mut self.scene_configs
    }

    pub fn snapshot(&self) -> ShowDocument {
        ShowDocument {
            lockout: self.lockout,
            scene_configs: self.scene_configs.clone(),
            cued_scene_internal_id: self.cued_scene_internal_id,
        }
    }

    pub fn projection_state(&self) -> super::events::ShowProjectionState {
        let show_file_name = self
            .show_file_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| "Untitled Session".to_string());

        super::events::ShowProjectionState {
            lockout: self.lockout,
            scene_configs: self.scene_configs.clone(),
            cued_scene_id: self.cued_scene_internal_id.map(|id| id.to_string()),
            selected_scene_id: self.selected_scene_id.clone(),
            show_file_path: self.show_file_path.clone(),
            show_file_name,
            show_file_dirty: self.show_file_dirty,
            show_file_last_saved_at: self.show_file_last_saved_at.clone(),
            discovered_lv1_systems: self.discovered_lv1_systems.clone(),
            connected_lv1_identity: self.connected_lv1_identity.clone(),
            pending_lv1_identity: self.pending_lv1_identity.clone(),
            reconnect: self.reconnect.clone(),
            last_event_at: self.last_event_at.clone(),
        }
    }

    pub fn cue_scene(&mut self, scene_id: &str) -> Result<bool, String> {
        if !self
            .scene_configs
            .iter()
            .any(|scene| scene.internal_scene_id.to_string() == scene_id)
        {
            return Err("Scene config not found".to_string());
        }

        let next = Some(scene_id.to_string());
        if self.selected_scene_id == next {
            return Ok(false);
        }

        self.cued_scene_internal_id = self
            .scene_configs
            .iter()
            .find(|scene| scene.internal_scene_id.to_string() == scene_id)
            .map(|scene| scene.internal_scene_id)
            .or_else(|| Some(scene_internal_id(scene_id)));
        Ok(true)
    }

    pub fn reconcile_scene_fade_configs(&mut self, scenes: &[SceneListEntry]) -> bool {
        let old_entries = entries_from_configs(&self.scene_configs);
        let new_entries = entries_from_scene_list(scenes);

        let changed = match classify_scene_list_change(&old_entries, &new_entries) {
            SceneListChange::Noop => false,
            SceneListChange::Rename => self.apply_position_mapping(&new_entries),
            SceneListChange::Move { .. }
            | SceneListChange::Insert { .. }
            | SceneListChange::Delete { .. }
            | SceneListChange::Ambiguous => {
                self.reconcile_scene_fade_configs_by_name_fifo(&new_entries)
            }
        };

        if changed {
            self.clear_missing_cue();
        }

        changed
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
        let unlinked_configs = old_configs
            .iter()
            .filter(|scene| scene.scene_index.is_none())
            .cloned()
            .collect::<Vec<_>>();
        for scene in old_configs
            .iter()
            .filter(|scene| scene.scene_index.is_some())
            .cloned()
        {
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
        next.extend(unlinked_configs);

        let changed = next != old_configs;
        self.scene_configs = next;
        changed
    }

    pub fn replace_snapshot(&mut self, snapshot: ShowDocument) {
        self.lockout = snapshot.lockout;
        self.scene_configs = snapshot.scene_configs;
        self.cued_scene_internal_id = snapshot.cued_scene_internal_id;
    }

    pub fn clear(&mut self) {
        self.lockout = false;
        self.scene_configs.clear();
        self.cued_scene_internal_id = None;
    }

    fn clear_missing_cue(&mut self) {
        if let Some(cued_scene_internal_id) = self.cued_scene_internal_id
            && !self
                .scene_configs
                .iter()
                .any(|scene| scene.internal_scene_id == cued_scene_internal_id)
        {
            self.cued_scene_internal_id = None;
        }
    }

    pub fn get_scene_config(&self, scene_id: &str) -> Option<SceneConfig> {
        self.scene_configs
            .iter()
            .find(|scene| scene.internal_scene_id.to_string() == scene_id)
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
            .find(|scene| scene.internal_scene_id.to_string() == scene_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::{ChannelInfo, PanMode};
    use crate::show::ChannelConfig;
    use uuid::Uuid;

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

    fn test_uuid(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn scene_config(
        scene_index: i32,
        scene_name: &str,
        duration_ms: u64,
        channels: Vec<ChannelConfig>,
        internal_scene_id: Uuid,
    ) -> SceneConfig {
        SceneConfig {
            internal_scene_id,
            scene_index: Some(scene_index),
            scene_name: scene_name.to_string(),
            duration_ms,
            channel_configs: channels,
            scoped_channels: vec![],
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    fn named_scene_config(index: i32, name: &str, duration_ms: u64) -> SceneConfig {
        scene_config(
            index,
            name,
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
            Uuid::from_u128((index as u128) + 1),
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
                1,
                "Verse",
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
                Uuid::from_u128(0x11111111111141118111111111111111),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(state.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 1,
            name: "Verse Big".to_string(),
        }]));

        assert_eq!(state.scene_configs.len(), 1);
        assert_eq!(state.scene_configs[0].scene_index, Some(1));
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
                1,
                "scene-1",
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
                Uuid::from_u128(0x22222222222242228222222222222222),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(!state.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 1,
            name: "scene-1".to_string(),
        }]));

        assert_eq!(state.scene_configs[0].duration_ms, 1_500);
        assert_eq!(state.scene_configs[0].scene_index, Some(1));
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
                scene_config(
                    2,
                    "Chorus",
                    300,
                    vec![],
                    Uuid::from_u128(0x33333333333343338333333333333333),
                ),
                scene_config(
                    0,
                    "Intro",
                    100,
                    vec![],
                    Uuid::from_u128(0x44444444444444448444444444444444),
                ),
                scene_config(
                    1,
                    "Verse",
                    200,
                    vec![],
                    Uuid::from_u128(0x55555555555545558555555555555555),
                ),
            ],
            cued_scene_internal_id: None,
            ..Default::default()
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

        assert_eq!(state.scene_configs[0].scene_index, Some(1));
        assert_eq!(state.scene_configs[1].scene_index, Some(0));
        assert_eq!(state.scene_configs[2].scene_index, Some(2));
    }

    #[test]
    fn reconciliation_reports_noop_when_scene_list_matches_current_configs() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                1,
                "scene-1",
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
                Uuid::from_u128(0x66666666666646668666666666666666),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
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
                1,
                "scene-1",
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
                Uuid::from_u128(0xdddddddddddddddddddddddddddddddd),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(state.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 2,
            name: "scene-1".to_string(),
        }]));
        assert_eq!(state.scene_configs.len(), 1);
        assert_eq!(state.scene_configs[0].scene_index, Some(2));
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
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(state.reconcile_scene_fade_configs(&[
            scene_entry(0, "Verse"),
            scene_entry(1, "Chorus"),
            scene_entry(2, "Intro"),
        ]));

        assert_eq!(state.scene_configs[0].scene_index, Some(0));
        assert_eq!(state.scene_configs[0].scene_name, "Verse");
        assert_eq!(state.scene_configs[0].duration_ms, 200);
        assert_eq!(state.scene_configs[1].scene_index, Some(1));
        assert_eq!(state.scene_configs[1].scene_name, "Chorus");
        assert_eq!(state.scene_configs[1].duration_ms, 300);
        assert_eq!(state.scene_configs[2].scene_index, Some(2));
        assert_eq!(state.scene_configs[2].scene_name, "Intro");
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
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(state.reconcile_scene_fade_configs(&[
            scene_entry(0, "Chorus"),
            scene_entry(1, "Intro"),
            scene_entry(2, "Verse"),
        ]));

        assert_eq!(state.scene_configs[0].scene_index, Some(0));
        assert_eq!(state.scene_configs[0].scene_name, "Chorus");
        assert_eq!(state.scene_configs[0].duration_ms, 300);
        assert_eq!(state.scene_configs[1].scene_index, Some(1));
        assert_eq!(state.scene_configs[1].scene_name, "Intro");
        assert_eq!(state.scene_configs[1].duration_ms, 100);
        assert_eq!(state.scene_configs[2].scene_index, Some(2));
        assert_eq!(state.scene_configs[2].scene_name, "Verse");
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
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(state.reconcile_scene_fade_configs(&[
            scene_entry(0, "Intro"),
            scene_entry(1, "Verse"),
            scene_entry(2, "Chorus"),
        ]));

        assert_eq!(state.scene_configs[0].scene_index, Some(0));
        assert_eq!(state.scene_configs[0].scene_name, "Intro");
        assert_eq!(state.scene_configs[0].duration_ms, 100);
        assert_eq!(state.scene_configs[1].scene_index, Some(1));
        assert_eq!(state.scene_configs[1].scene_name, "Verse");
        assert_eq!(state.scene_configs[1].duration_ms, 0);
        assert!(state.scene_configs[1].channel_configs.is_empty());
        assert_eq!(state.scene_configs[2].scene_index, Some(2));
        assert_eq!(state.scene_configs[2].scene_name, "Chorus");
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
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(
            state
                .reconcile_scene_fade_configs(&[scene_entry(0, "Intro"), scene_entry(1, "Chorus")])
        );

        assert_eq!(state.scene_configs.len(), 2);
        assert_eq!(state.scene_configs[0].scene_index, Some(0));
        assert_eq!(state.scene_configs[0].scene_name, "Intro");
        assert_eq!(state.scene_configs[0].duration_ms, 100);
        assert_eq!(state.scene_configs[1].scene_index, Some(1));
        assert_eq!(state.scene_configs[1].scene_name, "Chorus");
        assert_eq!(state.scene_configs[1].duration_ms, 300);
    }

    #[test]
    fn reconciliation_clears_missing_cued_scene_id() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                named_scene_config(0, "Intro", 100),
                named_scene_config(1, "Verse", 200),
            ],
            cued_scene_internal_id: Some(Uuid::from_u128(0x77777777777747778777777777777777)),
            ..Default::default()
        };

        assert!(state.reconcile_scene_fade_configs(&[scene_entry(0, "Intro")]));

        assert_eq!(state.cued_scene_internal_id, None);
    }

    #[test]
    fn reconciliation_uses_exact_match_fallback_for_multi_operation_change() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                named_scene_config(0, "Intro", 100),
                named_scene_config(1, "Verse", 200),
            ],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(state.reconcile_scene_fade_configs(&[
            scene_entry(0, "Intro New"),
            scene_entry(1, "Verse New")
        ]));

        assert_eq!(state.scene_configs[0].scene_index, Some(0));
        assert_eq!(state.scene_configs[0].scene_name, "Intro New");
        assert_eq!(state.scene_configs[0].duration_ms, 0);
        assert_eq!(state.scene_configs[1].scene_index, Some(1));
        assert_eq!(state.scene_configs[1].scene_name, "Verse New");
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
            cued_scene_internal_id: None,
            ..Default::default()
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
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(state.reconcile_scene_fade_configs(&[
            scene_entry(0, "Verse"),
            scene_entry(1, "Intro"),
            scene_entry(2, "Chorus"),
        ]));

        assert_eq!(state.scene_configs[0].scene_index, Some(0));
        assert_eq!(state.scene_configs[0].scene_name, "Verse");
        assert_eq!(state.scene_configs[0].duration_ms, 200);
        assert_eq!(state.scene_configs[1].scene_index, Some(1));
        assert_eq!(state.scene_configs[1].scene_name, "Intro");
        assert_eq!(state.scene_configs[1].duration_ms, 100);
        assert_eq!(state.scene_configs[2].scene_index, Some(2));
        assert_eq!(state.scene_configs[2].scene_name, "Chorus");
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
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(state.reconcile_scene_fade_configs(&[
            scene_entry(0, "Dupe"),
            scene_entry(1, "Intro"),
            scene_entry(2, "Dupe"),
            scene_entry(3, "Chorus"),
        ]));

        assert_eq!(state.scene_configs[0].scene_index, Some(0));
        assert_eq!(state.scene_configs[0].scene_name, "Dupe");
        assert_eq!(state.scene_configs[0].duration_ms, 200);
        assert_eq!(state.scene_configs[1].scene_index, Some(1));
        assert_eq!(state.scene_configs[1].scene_name, "Intro");
        assert_eq!(state.scene_configs[1].duration_ms, 100);
        assert_eq!(state.scene_configs[2].scene_index, Some(2));
        assert_eq!(state.scene_configs[2].scene_name, "Dupe");
        assert_eq!(state.scene_configs[2].duration_ms, 300);
        assert_eq!(state.scene_configs[3].scene_index, Some(3));
        assert_eq!(state.scene_configs[3].scene_name, "Chorus");
        assert_eq!(state.scene_configs[3].duration_ms, 400);
    }

    #[test]
    fn invalid_duration_rejected() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                1,
                "scene-1",
                1_000,
                vec![],
                Uuid::from_u128(0x88888888888848888888888888888888),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
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
                1,
                "scene-1",
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
                test_uuid(0x99999999999949999999999999999999),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        let scene_id = state.scene_configs[0].internal_scene_id.to_string();

        assert!(state.set_channel_scoped(&scene_id, 0, 1, true).unwrap());
        assert!(!state.set_channel_scoped(&scene_id, 0, 1, true).unwrap());
    }

    #[test]
    fn store_scene_config_snapshots_current_channels_and_scopes_first_store() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![],
            cued_scene_internal_id: None,
            ..Default::default()
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

        let scene_id = test_uuid(0x11111111111141118111111111111111).to_string();

        assert!(state.store_scene_config(&scene_id, &channels).unwrap());

        let snapshot = state.snapshot();
        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(snapshot.scene_configs[0].scene_index, None);
        assert_eq!(snapshot.scene_configs[0].scene_name, scene_id);
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
    fn store_scene_config_creates_missing_uuid_scene_config() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![],
            cued_scene_internal_id: None,
            ..Default::default()
        };
        let channels = vec![channel(0, 1, "Lead", -7.5)];

        let scene_id = test_uuid(0xabcabcabcabc4abc8abcabcabcabcabc).to_string();

        assert!(state.store_scene_config(&scene_id, &channels).unwrap());
        let snapshot = state.snapshot();
        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(snapshot.scene_configs[0].scene_name, scene_id);
        assert_eq!(snapshot.scene_configs[0].scene_index, None);
    }

    #[test]
    fn store_scene_config_keeps_scene_name_for_missing_uuid() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![],
            cued_scene_internal_id: None,
            ..Default::default()
        };
        let channels = vec![channel(0, 1, "Lead", -7.5)];

        let scene_id = test_uuid(0xdefdefdefdef4def8defdefdefdefdef).to_string();

        assert!(state.store_scene_config(&scene_id, &channels).unwrap());
        let snapshot = state.snapshot();
        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(snapshot.scene_configs[0].scene_name, scene_id);
    }

    #[test]
    fn store_scene_config_preserves_existing_pan_family_fields() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                1,
                "scene-1",
                1_000,
                vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-9.0),
                    pan: Some(-12.0),
                    balance: Some(3.0),
                    width: Some(1.2),
                    pan_mode: Some(PanMode::Stereo),
                }],
                test_uuid(0xaaaaaaaaaaaa4aaaaaaaaaaaaaaaaaaa),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        let scene_id = state.scene_configs[0].internal_scene_id.to_string();

        assert!(
            state
                .store_scene_config(&scene_id, &[channel(0, 1, "Lead", -6.0)])
                .unwrap()
        );

        let stored = &state.scene_configs[0].channel_configs[0];
        assert_eq!(stored.fader_db, Some(-6.0));
        assert_eq!(stored.pan, Some(-12.0));
        assert_eq!(stored.balance, Some(3.0));
        assert_eq!(stored.width, Some(1.2));
        assert_eq!(stored.pan_mode, Some(PanMode::Stereo));
    }

    #[test]
    fn store_scene_config_updates_fresh_pan_family_fields_when_available() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                1,
                "scene-1",
                1_000,
                vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-9.0),
                    pan: Some(-12.0),
                    balance: Some(3.0),
                    width: Some(1.2),
                    pan_mode: Some(PanMode::Stereo),
                }],
                test_uuid(0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        let scene_id = state.scene_configs[0].internal_scene_id.to_string();

        assert!(
            state
                .store_scene_config(
                    &scene_id,
                    &[ChannelInfo {
                        group: 0,
                        channel: 1,
                        name: "Lead".to_string(),
                        gain_db: -6.0,
                        muted: false,
                        pan: Some(0.25),
                        balance: Some(-0.5),
                        width: Some(1.0),
                        pan_mode: Some(PanMode::Mono),
                    }]
                )
                .unwrap()
        );

        let stored = &state.scene_configs[0].channel_configs[0];
        assert_eq!(stored.fader_db, Some(-6.0));
        assert_eq!(stored.pan, Some(0.25));
        assert_eq!(stored.balance, Some(-0.5));
        assert_eq!(stored.width, Some(1.0));
        assert_eq!(stored.pan_mode, Some(PanMode::Mono));
    }

    #[test]
    fn store_scene_config_preserves_existing_pan_family_fields_when_live_values_missing() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                1,
                "scene-1",
                1_000,
                vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-9.0),
                    pan: Some(-12.0),
                    balance: Some(3.0),
                    width: Some(1.2),
                    pan_mode: Some(PanMode::Stereo),
                }],
                test_uuid(0xcccccccccccccccccccccccccccccccc),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        let scene_id = state.scene_configs[0].internal_scene_id.to_string();

        assert!(
            state
                .store_scene_config(
                    &scene_id,
                    &[ChannelInfo {
                        group: 0,
                        channel: 1,
                        name: "Lead".to_string(),
                        gain_db: -6.0,
                        muted: false,
                        pan: None,
                        balance: None,
                        width: Some(2.0),
                        pan_mode: None,
                    }]
                )
                .unwrap()
        );

        let stored = &state.scene_configs[0].channel_configs[0];
        assert_eq!(stored.fader_db, Some(-6.0));
        assert_eq!(stored.pan, Some(-12.0));
        assert_eq!(stored.balance, Some(3.0));
        assert_eq!(stored.width, Some(2.0));
        assert_eq!(stored.pan_mode, Some(PanMode::Stereo));
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
        let scene_id = test_uuid(0xdddddddddddd4ddddddddddddddddddd).to_string();
        state
            .store_scene_config(&scene_id, &[channel(0, 1, "Lead", -6.0)])
            .unwrap();

        let scene_id = state.scene_configs[0].internal_scene_id.to_string();
        assert!(
            state
                .set_scene_scope_faders_enabled(&scene_id, false)
                .unwrap()
        );

        state
            .store_scene_config(&scene_id, &[channel(0, 1, "Lead", -3.0)])
            .unwrap();

        assert!(!state.scene_configs[0].scope_toggles.faders);
    }

    #[test]
    fn scene_scope_fader_toggle_mutation_reports_noop() {
        let mut state = ShowState::default();
        let scene_id = test_uuid(0xeeeeeeeeeeee4eeeeeeeeeeeeeeeeeee).to_string();
        state
            .store_scene_config(&scene_id, &[channel(0, 1, "Lead", -6.0)])
            .unwrap();

        let scene_id = state.scene_configs[0].internal_scene_id.to_string();

        assert!(
            state
                .set_scene_scope_faders_enabled(&scene_id, false)
                .unwrap()
        );
        assert!(
            !state
                .set_scene_scope_faders_enabled(&scene_id, false)
                .unwrap()
        );
        assert!(!state.scene_configs[0].scope_toggles.faders);
    }

    #[test]
    fn scene_scope_pan_toggle_mutation_reports_noop() {
        let mut state = ShowState::default();
        let scene_id = test_uuid(0xffffffffffff4fffffffffffffffffff).to_string();
        state
            .store_scene_config(&scene_id, &[channel(0, 1, "Lead", -6.0)])
            .unwrap();

        let scene_id = state.scene_configs[0].internal_scene_id.to_string();

        assert!(state.set_scene_scope_pan_enabled(&scene_id, true).unwrap());
        assert!(!state.set_scene_scope_pan_enabled(&scene_id, true).unwrap());
        assert!(state.scene_configs[0].scope_toggles.pan);
    }

    #[test]
    fn scene_scope_pan_toggle_requires_existing_scene_config() {
        let mut state = ShowState::default();

        let err = state
            .set_scene_scope_pan_enabled("missing", false)
            .unwrap_err();

        assert_eq!(err, "Scene config not found");
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
        let scene_id = test_uuid(0xabababababab4abababababababababa).to_string();
        state
            .store_scene_config(&scene_id, &[channel(0, 1, "Lead", -6.0)])
            .unwrap();

        let scene_id = state.scene_configs[0].internal_scene_id.to_string();

        assert!(state.set_scene_duration_ms(&scene_id, 0).unwrap());
        assert_eq!(state.scene_configs[0].duration_ms, 0);
    }
}

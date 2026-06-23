use std::collections::{HashMap, HashSet, VecDeque};

use crate::lv1::SceneListEntry;
use uuid::Uuid;

use super::types::{SceneConfig, SceneScopeToggles};

#[derive(Debug, Clone, PartialEq, Eq)]
struct SceneEntry {
    index: i32,
    name: String,
}

fn entries_from_configs(configs: &[SceneConfig]) -> Vec<SceneEntry> {
    configs
        .iter()
        .filter_map(|scene| {
            scene.scene_index.map(|index| SceneEntry {
                index,
                name: scene.scene_name.clone(),
            })
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

fn apply_position_mapping(configs: Vec<SceneConfig>, entries: &[SceneEntry]) -> Vec<SceneConfig> {
    let mut linked = configs
        .iter()
        .filter(|scene| scene.scene_index.is_some())
        .cloned()
        .collect::<Vec<_>>();
    let mut unlinked = configs
        .into_iter()
        .filter(|scene| scene.scene_index.is_none())
        .collect::<Vec<_>>();
    for (config, entry) in linked.iter_mut().zip(entries.iter()) {
        update_scene_locator(config, entry);
    }
    for scene in unlinked.iter_mut() {
        scene.scene_index = None;
    }
    linked.extend(unlinked);
    linked
}

fn align_by_name_fifo(configs: Vec<SceneConfig>, entries: &[SceneEntry]) -> Vec<SceneConfig> {
    let mut old_by_name = HashMap::<String, VecDeque<SceneConfig>>::new();
    let mut old_linked = Vec::new();
    let mut leftovers = Vec::new();
    for scene in configs {
        if scene.scene_index.is_some() {
            old_by_name
                .entry(scene.scene_name.clone())
                .or_default()
                .push_back(scene.clone());
            old_linked.push(scene);
        } else {
            leftovers.push(scene);
        }
    }

    let mut next = Vec::with_capacity(entries.len() + leftovers.len());
    let mut consumed = HashSet::new();
    for entry in entries {
        let mut scene = old_by_name
            .get_mut(&entry.name)
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(|| default_scene_config(entry));
        update_scene_locator(&mut scene, entry);
        consumed.insert(scene.internal_scene_id);
        next.push(scene);
    }
    for mut scene in old_linked {
        if !consumed.contains(&scene.internal_scene_id) {
            scene.scene_index = None;
            next.push(scene);
        }
    }
    for mut scene in leftovers {
        scene.scene_index = None;
        next.push(scene);
    }
    next
}

pub(crate) fn align_scene_configs(
    configs: Vec<SceneConfig>,
    lv1_scenes: &[SceneListEntry],
) -> Vec<SceneConfig> {
    let old_entries = entries_from_configs(&configs);
    let new_entries = entries_from_scene_list(lv1_scenes);
    match classify_scene_list_change(&old_entries, &new_entries) {
        SceneListChange::Noop => configs,
        SceneListChange::Rename => apply_position_mapping(configs, &new_entries),
        SceneListChange::Move { .. }
        | SceneListChange::Insert { .. }
        | SceneListChange::Delete { .. }
        | SceneListChange::Ambiguous => align_by_name_fifo(configs, &new_entries),
    }
}

pub(crate) fn scene_alignment_diagnostic(
    old: &[SceneConfig],
    new: &[SceneConfig],
    lv1_scenes: &[SceneListEntry],
) -> String {
    let old_entries = entries_from_configs(old);
    let new_entries = entries_from_configs(new);
    let lv1_entries = entries_from_scene_list(lv1_scenes);
    let change = classify_scene_list_change(&old_entries, &lv1_entries);
    let candidates = move_candidates(&old_entries, &lv1_entries)
        .into_iter()
        .map(|(from, to)| format!("{from}->{to}"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "scene alignment preview: change={} old=[{}] new=[{}] move_candidates=[{}] duplicate_names=[{}] strategy={}",
        change.diagnostic_label(),
        describe_entries(&old_entries),
        describe_entries(&new_entries),
        candidates,
        name_counts(&lv1_entries).join(","),
        if matches!(change, SceneListChange::Rename | SceneListChange::Noop) {
            "classified"
        } else {
            "name-keyed-fifo"
        },
    )
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::lv1::SceneListEntry;

    use super::{align_scene_configs, scene_alignment_diagnostic};
    use crate::show::{SceneConfig, SceneScopeToggles};

    fn scene(id: u128, index: Option<i32>, name: &str, duration_ms: u64) -> SceneConfig {
        SceneConfig {
            internal_scene_id: Uuid::from_u128(id),
            scene_index: index,
            scene_name: name.to_string(),
            duration_ms,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    fn lv1_scene(index: i32, name: &str) -> SceneListEntry {
        SceneListEntry {
            index,
            name: name.to_string(),
        }
    }

    #[test]
    fn exact_index_name_match_preserves_uuid_and_fade_data() {
        let old = vec![scene(1, Some(1), "Verse", 1_500)];
        let new = vec![lv1_scene(1, "Verse")];

        let aligned = align_scene_configs(old.clone(), &new);

        assert_eq!(aligned, old);
    }

    #[test]
    fn single_same_index_rename_preserves_uuid_and_updates_name() {
        let old = vec![scene(1, Some(1), "Verse", 1_500)];
        let new = vec![lv1_scene(1, "Verse Big")];

        let aligned = align_scene_configs(old, &new);

        assert_eq!(aligned[0].internal_scene_id, Uuid::from_u128(1));
        assert_eq!(aligned[0].scene_index, Some(1));
        assert_eq!(aligned[0].scene_name, "Verse Big");
        assert_eq!(aligned[0].duration_ms, 1_500);
    }

    #[test]
    fn single_move_preserves_uuid_and_updates_index() {
        let old = vec![
            scene(1, Some(1), "Intro", 1_000),
            scene(2, Some(2), "Verse", 2_000),
        ];
        let new = vec![lv1_scene(2, "Verse"), lv1_scene(1, "Intro")];

        let aligned = align_scene_configs(old, &new);

        assert_eq!(aligned[0].internal_scene_id, Uuid::from_u128(2));
        assert_eq!(aligned[0].scene_index, Some(2));
        assert_eq!(aligned[1].internal_scene_id, Uuid::from_u128(1));
        assert_eq!(aligned[1].scene_index, Some(1));
    }

    #[test]
    fn single_insert_creates_one_default_linked_config() {
        let old = vec![scene(1, Some(1), "Intro", 1_000)];
        let new = vec![lv1_scene(1, "Intro"), lv1_scene(2, "Verse")];

        let aligned = align_scene_configs(old, &new);

        assert_eq!(aligned.len(), 2);
        assert_eq!(aligned[1].scene_index, Some(2));
        assert_eq!(aligned[1].scene_name, "Verse");
        assert_eq!(aligned[1].duration_ms, 0);
        assert!(aligned[1].channel_configs.is_empty());
    }

    #[test]
    fn single_delete_preserves_deleted_config_as_unlinked() {
        let old = vec![
            scene(1, Some(1), "Intro", 1_000),
            scene(2, Some(2), "Verse", 2_000),
        ];
        let new = vec![lv1_scene(1, "Intro")];

        let aligned = align_scene_configs(old, &new);

        assert_eq!(aligned.len(), 2);
        assert_eq!(aligned[0].scene_index, Some(1));
        assert_eq!(aligned[1].scene_index, None);
        assert_eq!(aligned[1].scene_name, "Verse");
    }

    #[test]
    fn deleted_configs_are_preserved_in_original_order() {
        let old = vec![
            scene(1, Some(1), "Intro", 1_000),
            scene(2, Some(2), "Bridge", 2_000),
            scene(3, Some(3), "Verse", 3_000),
            scene(4, Some(4), "Chorus", 4_000),
        ];
        let new = vec![lv1_scene(1, "Intro")];

        let aligned = align_scene_configs(old, &new);

        assert_eq!(aligned[1].internal_scene_id, Uuid::from_u128(2));
        assert_eq!(aligned[2].internal_scene_id, Uuid::from_u128(3));
        assert_eq!(aligned[3].internal_scene_id, Uuid::from_u128(4));
        assert!(aligned[1..].iter().all(|scene| scene.scene_index.is_none()));
    }

    #[test]
    fn ambiguous_multi_rename_unlinks_old_and_defaults_new() {
        let old = vec![scene(1, Some(1), "A", 1_000), scene(2, Some(2), "B", 2_000)];
        let new = vec![lv1_scene(1, "A2"), lv1_scene(2, "B2")];

        let aligned = align_scene_configs(old, &new);

        assert_eq!(aligned[0].scene_index, Some(1));
        assert_eq!(aligned[0].scene_name, "A2");
        assert_eq!(aligned[1].scene_index, Some(2));
        assert_eq!(aligned[1].scene_name, "B2");
    }

    #[test]
    fn existing_unlinked_config_remains_unlinked() {
        let old = vec![
            scene(1, Some(1), "Intro", 1_000),
            scene(2, None, "Draft", 2_000),
        ];
        let new = vec![lv1_scene(1, "Intro")];

        let aligned = align_scene_configs(old, &new);

        assert_eq!(aligned[0].scene_index, Some(1));
        assert_eq!(aligned[1].scene_index, None);
    }

    #[test]
    fn unlinked_config_before_rename_remains_unlinked() {
        let old = vec![
            scene(1, None, "Draft", 2_000),
            scene(2, Some(1), "Verse", 1_000),
        ];
        let new = vec![lv1_scene(1, "Verse Big")];

        let aligned = align_scene_configs(old, &new);

        assert_eq!(aligned[0].internal_scene_id, Uuid::from_u128(2));
        assert_eq!(aligned[0].scene_index, Some(1));
        assert_eq!(aligned[0].scene_name, "Verse Big");
        assert_eq!(aligned[1].internal_scene_id, Uuid::from_u128(1));
        assert_eq!(aligned[1].scene_index, None);
        assert_eq!(aligned[1].scene_name, "Draft");
    }

    #[test]
    fn duplicate_names_keep_fifo_for_classified_move() {
        let old = vec![
            scene(1, Some(1), "Intro", 1_000),
            scene(2, Some(2), "Intro", 2_000),
        ];
        let new = vec![lv1_scene(2, "Intro"), lv1_scene(1, "Intro")];

        let aligned = align_scene_configs(old, &new);

        assert_eq!(aligned[0].internal_scene_id, Uuid::from_u128(1));
        assert_eq!(aligned[1].internal_scene_id, Uuid::from_u128(2));
    }

    #[test]
    fn diagnostic_is_string_only() {
        let old = vec![scene(1, Some(1), "Intro", 1_000)];
        let new = vec![scene(1, Some(1), "Intro", 1_000)];

        let diagnostic = scene_alignment_diagnostic(&old, &new, &[lv1_scene(1, "Intro")]);

        assert!(diagnostic.contains("scene alignment"));
    }
}

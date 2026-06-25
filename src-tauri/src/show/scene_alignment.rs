use std::collections::HashMap;

use crate::lv1::SceneListEntry;
use uuid::Uuid;

use crate::scenes::{SceneConfig, SceneScopeToggles};

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

fn describe_entries(entries: &[SceneEntry]) -> String {
    entries
        .iter()
        .map(|entry| format!("{}:{:?}", entry.index, entry.name))
        .collect::<Vec<_>>()
        .join(" | ")
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

fn scene_entry_name_counts(entries: &[SceneEntry]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for entry in entries {
        *counts.entry(entry.name.clone()).or_default() += 1;
    }
    counts
}

fn align_by_current_lv1_order(
    configs: Vec<SceneConfig>,
    entries: &[SceneEntry],
) -> Vec<SceneConfig> {
    let mut remaining = configs
        .iter()
        .filter(|scene| scene.scene_index.is_some())
        .cloned()
        .collect::<Vec<_>>();
    let mut unlinked = configs
        .into_iter()
        .filter(|scene| scene.scene_index.is_none())
        .collect::<Vec<_>>();
    let old_name_counts = scene_config_name_counts(&remaining);
    let new_name_counts = scene_entry_name_counts(entries);
    let renamed_index = single_same_index_rename(&remaining, entries);
    let mut next = Vec::with_capacity(entries.len() + remaining.len());
    for entry in entries {
        if let Some(position) = remaining.iter().position(|scene| {
            scene.scene_index == Some(entry.index) && scene.scene_name == entry.name
        }) {
            let mut scene = remaining.remove(position);
            update_scene_locator(&mut scene, entry);
            next.push(scene);
        } else if let Some(position) =
            unique_name_match_position(&remaining, entry, &old_name_counts, &new_name_counts)
        {
            let mut scene = remaining.remove(position);
            update_scene_locator(&mut scene, entry);
            next.push(scene);
        } else if renamed_index == Some(entry.index) {
            if let Some(position) = remaining
                .iter()
                .position(|scene| scene.scene_index == Some(entry.index))
            {
                let mut scene = remaining.remove(position);
                update_scene_locator(&mut scene, entry);
                next.push(scene);
            } else {
                next.push(default_scene_config(entry));
            }
        } else {
            next.push(default_scene_config(entry));
        }
    }
    for mut scene in remaining {
        scene.scene_index = None;
        next.push(scene);
    }
    for scene in unlinked.iter_mut() {
        scene.scene_index = None;
    }
    next.extend(unlinked);
    next
}

fn unique_name_match_position(
    remaining: &[SceneConfig],
    entry: &SceneEntry,
    old_name_counts: &HashMap<String, usize>,
    new_name_counts: &HashMap<String, usize>,
) -> Option<usize> {
    if old_name_counts.get(&entry.name) == Some(&1) && new_name_counts.get(&entry.name) == Some(&1)
    {
        remaining
            .iter()
            .position(|scene| scene.scene_name == entry.name)
    } else {
        None
    }
}

fn single_same_index_rename(configs: &[SceneConfig], entries: &[SceneEntry]) -> Option<i32> {
    if configs.len() != entries.len() {
        return None;
    }
    let mut renamed = None;
    for scene in configs {
        let entry = entries
            .iter()
            .find(|entry| scene.scene_index == Some(entry.index))?;
        if scene.scene_name != entry.name {
            if renamed.is_some() {
                return None;
            }
            renamed = Some(entry.index);
        }
    }
    renamed
}

fn scene_config_name_counts(configs: &[SceneConfig]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for scene in configs {
        *counts.entry(scene.scene_name.clone()).or_default() += 1;
    }
    counts
}

pub(crate) fn align_scene_configs(
    configs: Vec<SceneConfig>,
    lv1_scenes: &[SceneListEntry],
) -> Vec<SceneConfig> {
    align_by_current_lv1_order(configs, &entries_from_scene_list(lv1_scenes))
}

pub(crate) fn scene_alignment_diagnostic(
    old: &[SceneConfig],
    new: &[SceneConfig],
    lv1_scenes: &[SceneListEntry],
) -> String {
    let old_entries = entries_from_configs(old);
    let new_entries = entries_from_configs(new);
    let lv1_entries = entries_from_scene_list(lv1_scenes);
    format!(
        "scene alignment preview: old=[{}] new=[{}] lv1=[{}] duplicate_names=[{}] strategy=exact-unique-name-single-rename",
        describe_entries(&old_entries),
        describe_entries(&new_entries),
        describe_entries(&lv1_entries),
        name_counts(&lv1_entries).join(","),
    )
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::lv1::SceneListEntry;

    use super::{align_scene_configs, scene_alignment_diagnostic};
    use crate::scenes::{SceneConfig, SceneScopeToggles};

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
    fn block_move_preserves_configs_by_unique_names() {
        let old = vec![
            scene(1, Some(1), "Song 1", 1_000),
            scene(2, Some(2), "Song 2", 2_000),
        ];
        let new = vec![lv1_scene(5, "Song 1"), lv1_scene(6, "Song 2")];

        let aligned = align_scene_configs(old, &new);

        assert_eq!(aligned.len(), 2);
        assert_eq!(aligned[0].internal_scene_id, Uuid::from_u128(1));
        assert_eq!(aligned[0].scene_index, Some(5));
        assert_eq!(aligned[0].duration_ms, 1_000);
        assert_eq!(aligned[1].internal_scene_id, Uuid::from_u128(2));
        assert_eq!(aligned[1].scene_index, Some(6));
        assert_eq!(aligned[1].duration_ms, 2_000);
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

        assert_eq!(aligned.len(), 4);
        assert_ne!(aligned[0].internal_scene_id, Uuid::from_u128(1));
        assert_ne!(aligned[1].internal_scene_id, Uuid::from_u128(2));
        assert_eq!(aligned[0].scene_index, Some(1));
        assert_eq!(aligned[0].scene_name, "A2");
        assert_eq!(aligned[1].scene_index, Some(2));
        assert_eq!(aligned[1].scene_name, "B2");
        assert_eq!(aligned[2].internal_scene_id, Uuid::from_u128(1));
        assert_eq!(aligned[2].scene_index, None);
        assert_eq!(aligned[2].scene_name, "A");
        assert_eq!(aligned[3].internal_scene_id, Uuid::from_u128(2));
        assert_eq!(aligned[3].scene_index, None);
        assert_eq!(aligned[3].scene_name, "B");
    }

    #[test]
    fn ambiguous_duplicate_names_do_not_fifo_guess() {
        let old = vec![
            scene(1, Some(1), "Intro", 1_000),
            scene(2, Some(2), "Intro", 2_000),
        ];
        let new = vec![lv1_scene(1, "Intro"), lv1_scene(3, "Intro")];

        let aligned = align_scene_configs(old, &new);

        assert_eq!(aligned.len(), 3);
        assert_eq!(aligned[0].internal_scene_id, Uuid::from_u128(1));
        assert_eq!(aligned[0].scene_index, Some(1));
        assert_ne!(aligned[1].internal_scene_id, Uuid::from_u128(2));
        assert_eq!(aligned[1].scene_index, Some(3));
        assert_eq!(aligned[2].internal_scene_id, Uuid::from_u128(2));
        assert_eq!(aligned[2].scene_index, None);
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
    fn unlinked_config_with_unique_current_name_does_not_relink() {
        let old = vec![
            scene(1, None, "Song 1", 1_000),
            scene(2, Some(2), "Song 2", 2_000),
            scene(3, Some(3), "Song 3", 3_000),
        ];
        let new = vec![lv1_scene(5, "Song 1"), lv1_scene(6, "Song 2")];

        let aligned = align_scene_configs(old, &new);

        assert_ne!(aligned[0].internal_scene_id, Uuid::from_u128(1));
        assert_eq!(aligned[0].scene_index, Some(5));
        assert_eq!(aligned[1].internal_scene_id, Uuid::from_u128(2));
        assert_eq!(aligned[1].scene_index, Some(6));
        assert_eq!(aligned[2].internal_scene_id, Uuid::from_u128(3));
        assert_eq!(aligned[2].scene_index, None);
        assert_eq!(aligned[3].internal_scene_id, Uuid::from_u128(1));
        assert_eq!(aligned[3].scene_index, None);
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
    fn duplicate_names_preserve_exact_matches_before_default_insert() {
        let old = vec![
            scene(1, Some(1), "Intro", 1_000),
            scene(2, Some(2), "Intro", 2_000),
        ];
        let new = vec![
            lv1_scene(1, "Intro"),
            lv1_scene(2, "Intro"),
            lv1_scene(3, "Verse"),
        ];

        let aligned = align_scene_configs(old, &new);

        assert_eq!(aligned[0].internal_scene_id, Uuid::from_u128(1));
        assert_eq!(aligned[1].internal_scene_id, Uuid::from_u128(2));
        assert_eq!(aligned[2].scene_name, "Verse");
        assert_eq!(aligned[2].duration_ms, 0);
    }

    #[test]
    fn diagnostic_is_string_only() {
        let old = vec![scene(1, Some(1), "Intro", 1_000)];
        let new = vec![scene(1, Some(1), "Intro", 1_000)];

        let diagnostic = scene_alignment_diagnostic(&old, &new, &[lv1_scene(1, "Intro")]);

        assert!(diagnostic.contains("scene alignment"));
    }
}

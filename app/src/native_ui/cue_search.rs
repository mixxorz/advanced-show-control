use nucleo_matcher::{Config, Matcher, Utf32Str};
use uuid::Uuid;

use crate::scenes::SceneConfig;

use super::scene_library::format_scene_number;

/// @cc [owner:mixxorz,label:product] literal-number-first-cue-search
/// Search MUST return at most three existing scene IDs. Numeric queries MUST rank exact displayed
/// numbers before displayed-number prefixes and fuzzy names, without stripping leading zeroes.
/// Equal matches MUST retain library order except for the preferred identical-name scene.
pub(super) fn search_scenes(
    scenes: &[SceneConfig],
    query: &str,
    preferred_scene_id: Option<Uuid>,
) -> Vec<Uuid> {
    let query = query.trim();
    if query.is_empty() {
        return scenes
            .iter()
            .take(3)
            .map(|scene| scene.internal_scene_id)
            .collect();
    }

    let number_query = query.strip_prefix('#').unwrap_or(query);
    let number_query = (!number_query.is_empty()
        && number_query.bytes().all(|b| b.is_ascii_digit()))
    .then_some(number_query);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut needle_buffer = Vec::new();
    let needle = Utf32Str::new(query, &mut needle_buffer);
    let mut buffer = Vec::new();
    let mut ranked = Vec::new();

    for (position, scene) in scenes.iter().enumerate() {
        let number_rank = number_query.and_then(|number| {
            scene
                .scene_index
                .and_then(|index| (index >= 0).then(|| format_scene_number(Some(index))))
                .and_then(|displayed| {
                    if displayed == number {
                        Some(0u8)
                    } else if displayed.starts_with(number) {
                        Some(1)
                    } else {
                        None
                    }
                })
        });
        buffer.clear();
        let name = Utf32Str::new(&scene.scene_name, &mut buffer);
        let score = matcher.fuzzy_match(name, needle);
        if number_rank.is_some() || score.is_some() {
            let identical_name = scene.scene_name.eq_ignore_ascii_case(query);
            ranked.push((
                number_rank.unwrap_or(2),
                score.unwrap_or(0),
                identical_name && Some(scene.internal_scene_id) == preferred_scene_id,
                position,
                scene.internal_scene_id,
            ));
        }
    }
    ranked.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| a.3.cmp(&b.3))
    });
    ranked.into_iter().take(3).map(|entry| entry.4).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenes::SceneScopeToggles;

    fn scene(id: u128, index: Option<i32>, name: &str) -> SceneConfig {
        SceneConfig {
            internal_scene_id: Uuid::from_u128(id),
            scene_index: index,
            scene_name: name.into(),
            duration_ms: 0,
            channel_configs: vec![],
            scoped_channels: vec![],
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    fn ids(scenes: &[SceneConfig], query: &str, preferred: Option<u128>) -> Vec<Uuid> {
        search_scenes(scenes, query, preferred.map(Uuid::from_u128))
    }

    #[test]
    fn empty_query_returns_first_three_in_library_order() {
        let scenes = [
            scene(1, Some(9), "A"),
            scene(2, None, "B"),
            scene(3, None, "C"),
            scene(4, None, "D"),
        ];
        assert_eq!(
            ids(&scenes, "  ", Some(4)),
            vec![Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)]
        );
    }

    #[test]
    fn numbers_are_literal_padded_and_precede_fuzzy_names() {
        let scenes = [
            scene(1, Some(9), "Ten"),
            scene(2, Some(0), "010"),
            scene(3, Some(10), "Next"),
            scene(4, Some(99), "Other"),
        ];
        assert_eq!(
            ids(&scenes, "010", None),
            vec![Uuid::from_u128(1), Uuid::from_u128(2)]
        );
        assert_eq!(
            ids(&scenes, "#01", None),
            vec![Uuid::from_u128(1), Uuid::from_u128(3)]
        );
        assert_eq!(
            ids(&scenes, "10", None),
            vec![Uuid::from_u128(4), Uuid::from_u128(2)]
        );
        assert_eq!(ids(&scenes, "#010", None), vec![Uuid::from_u128(1)]);
    }

    #[test]
    fn fuzzy_names_accept_abbreviations_missing_characters_and_case() {
        let scenes = [
            scene(1, None, "Sound Check"),
            scene(2, None, "Opening Number"),
            scene(3, None, "Finale"),
        ];
        assert_eq!(ids(&scenes, "scheck", None), vec![Uuid::from_u128(1)]);
        assert_eq!(ids(&scenes, "opning", None), vec![Uuid::from_u128(2)]);
        assert_eq!(ids(&scenes, "FINALE", None), vec![Uuid::from_u128(3)]);
        assert!(ids(&scenes, "zzzzz", None).is_empty());
    }

    #[test]
    fn identical_names_prefer_current_scene_then_library_order() {
        let scenes = [
            scene(1, None, "Intro"),
            scene(2, None, "Intro"),
            scene(3, None, "Intro"),
        ];
        assert_eq!(
            ids(&scenes, "Intro", Some(3)),
            vec![Uuid::from_u128(3), Uuid::from_u128(1), Uuid::from_u128(2)]
        );
        assert_eq!(
            ids(&scenes, "Intro", None),
            vec![Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)]
        );
    }
}

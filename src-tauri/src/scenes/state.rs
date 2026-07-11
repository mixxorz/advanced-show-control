use std::time::Duration;

use tokio::time::Instant;

use crate::lv1::{SceneListEntry, SceneState};
use crate::scenes::scene_alignment::align_scene_configs;
use crate::scenes::{ChannelConfig, ChannelRef, SceneConfig, SceneDocument, SceneScopeToggles};

const RECALL_ARMING_DELAY: Duration = Duration::from_millis(2_000);
const SAME_SCENE_REPEAT_DELAY: Duration = Duration::from_millis(500);
const SCENE_LIST_EDIT_SUPPRESSION_WINDOW: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecallSceneIdentity {
    index: i32,
    name: String,
}

impl From<&SceneState> for RecallSceneIdentity {
    fn from(scene: &SceneState) -> Self {
        Self {
            index: scene.index,
            name: scene.name.clone(),
        }
    }
}

/// A scene observation: which scene and when it was seen.
#[derive(Debug, Clone)]
struct ObservedScene {
    scene: RecallSceneIdentity,
    at: Instant,
}

#[derive(Debug, Clone, PartialEq)]
struct SceneSettingsClipboard {
    duration_ms: u64,
    scope_toggles: SceneScopeToggles,
    channel_configs: Vec<ChannelConfig>,
    scoped_channels: Vec<ChannelRef>,
}

/// Gate deciding whether a scene observation is an operator recall.
///
/// The LV1 re-broadcasts the already-active scene around (re)connect, so the
/// gate arms over `RECALL_ARMING_DELAY`: scenes seen while arming are the
/// pre-existing scene (the baseline), not recalls. The same notify can land
/// again just after the window closes, so a baseline-equal scene observed
/// within `SAME_SCENE_REPEAT_DELAY` of the baseline observation is also
/// suppressed rather than treated as a recall.
#[derive(Debug, Default)]
enum RecallGate {
    /// No scene observed yet; the first observation starts arming.
    #[default]
    Unarmed,
    /// Within `RECALL_ARMING_DELAY` of the first observation. `baseline`
    /// tracks the most recently observed scene; the last one seen before the
    /// deadline is the pre-existing scene.
    Arming {
        baseline: ObservedScene,
        deadline: Instant,
    },
    /// Observations may trigger recalls, except baseline echoes and repeats
    /// of `last_trigger` within `SAME_SCENE_REPEAT_DELAY`.
    Armed {
        baseline: ObservedScene,
        last_trigger: Option<ObservedScene>,
    },
}

#[derive(Debug, Default)]
pub struct ScenesState {
    lockout: bool,
    scene_configs: Vec<SceneConfig>,
    selected_scene_internal_id: Option<String>,
    scene_settings_clipboard: Option<SceneSettingsClipboard>,
    gate: RecallGate,
    last_scene_list: Option<Vec<SceneListEntry>>,
    scene_list_edit_suppressed_until: Option<Instant>,
}

impl ScenesState {
    pub(crate) fn set_lockout(&mut self, lockout: bool) {
        self.lockout = lockout;
    }

    pub(crate) fn lockout(&self) -> bool {
        self.lockout
    }

    pub(crate) fn projection_state(&self) -> crate::scenes::ScenesProjectionState {
        crate::scenes::ScenesProjectionState {
            scene_configs: self.scene_configs.clone(),
            selected_scene_internal_id: self.selected_scene_internal_id.clone(),
            scene_settings_clipboard_available: self.scene_settings_clipboard.is_some(),
        }
    }

    pub(crate) fn snapshot(&self) -> SceneDocument {
        SceneDocument {
            scene_configs: self.scene_configs.clone(),
            selected_scene_internal_id: self.selected_scene_internal_id.clone(),
        }
    }

    pub(crate) fn replace_snapshot(&mut self, snapshot: SceneDocument) {
        self.scene_configs = snapshot.scene_configs;
        self.selected_scene_internal_id = snapshot.selected_scene_internal_id;
    }

    pub(crate) fn replace_snapshot_for_session(&mut self, snapshot: SceneDocument) {
        self.replace_snapshot(snapshot);
        self.scene_settings_clipboard = None;
        self.reset_recall_tracking();
    }

    pub(crate) fn get_scene_config(&self, internal_scene_id: uuid::Uuid) -> Option<SceneConfig> {
        self.scene_configs
            .iter()
            .find(|scene| scene.internal_scene_id == internal_scene_id)
            .cloned()
    }

    pub(crate) fn scene_configs(&self) -> &[SceneConfig] {
        &self.scene_configs
    }

    pub(crate) fn scene_list_entry_for_index(&self, index: i32) -> Option<SceneListEntry> {
        self.last_scene_list.as_ref().and_then(|scene_list| {
            scene_list
                .iter()
                .find(|scene| scene.index == index)
                .cloned()
        })
    }

    pub(crate) fn get_scene_config_mut(
        &mut self,
        internal_scene_id: uuid::Uuid,
    ) -> Option<&mut SceneConfig> {
        self.scene_configs
            .iter_mut()
            .find(|scene| scene.internal_scene_id == internal_scene_id)
    }

    pub(super) fn upsert_scene_config(&mut self, snapshot: SceneConfig) -> bool {
        match self
            .scene_configs
            .iter_mut()
            .find(|scene| scene.internal_scene_id == snapshot.internal_scene_id)
        {
            Some(existing) => {
                if *existing == snapshot {
                    false
                } else {
                    *existing = snapshot;
                    true
                }
            }
            None => {
                self.scene_configs.push(snapshot);
                true
            }
        }
    }

    pub(crate) fn select_scene_config(
        &mut self,
        internal_scene_id: uuid::Uuid,
    ) -> Result<bool, String> {
        if self.get_scene_config(internal_scene_id).is_none() {
            return Err("Scene config not found".to_string());
        }
        let next = Some(internal_scene_id.to_string());
        if self.selected_scene_internal_id == next {
            return Ok(false);
        }
        self.selected_scene_internal_id = next;
        Ok(true)
    }

    #[allow(dead_code)]
    pub(crate) fn copy_scene_settings(
        &mut self,
        source_internal_scene_id: uuid::Uuid,
    ) -> Result<bool, String> {
        let source = self
            .scene_configs
            .iter()
            .find(|scene| scene.internal_scene_id == source_internal_scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        let clipboard = SceneSettingsClipboard {
            duration_ms: source.duration_ms,
            scope_toggles: source.scope_toggles.clone(),
            channel_configs: source.channel_configs.clone(),
            scoped_channels: source.scoped_channels.clone(),
        };
        let changed = self.scene_settings_clipboard.as_ref() != Some(&clipboard);
        self.scene_settings_clipboard = Some(clipboard);
        Ok(changed)
    }

    #[allow(dead_code)]
    pub(crate) fn paste_scene_settings(
        &mut self,
        destination_internal_scene_id: uuid::Uuid,
    ) -> Result<bool, String> {
        let clipboard = self
            .scene_settings_clipboard
            .as_ref()
            .ok_or_else(|| "Scene settings clipboard is empty".to_string())?;
        let destination_index = self
            .scene_configs
            .iter()
            .position(|scene| scene.internal_scene_id == destination_internal_scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        let destination = &self.scene_configs[destination_index];
        if destination.scene_index.is_none() {
            return Err("Paste blocked: destination scene is not linked".to_string());
        }
        let prospective = SceneConfig {
            duration_ms: clipboard.duration_ms,
            scope_toggles: clipboard.scope_toggles.clone(),
            channel_configs: clipboard.channel_configs.clone(),
            scoped_channels: clipboard.scoped_channels.clone(),
            ..destination.clone()
        };
        if *destination == prospective {
            return Ok(false);
        }
        self.scene_configs[destination_index] = prospective;
        Ok(true)
    }

    pub(crate) fn link_scene_config(
        &mut self,
        source_internal_scene_id: uuid::Uuid,
        target: &SceneListEntry,
        overwrite_existing: bool,
    ) -> Result<bool, String> {
        let source = self
            .scene_configs
            .iter()
            .find(|scene| scene.internal_scene_id == source_internal_scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        if source.scene_index.is_some() {
            return Err("Link blocked: source scene is already linked".to_string());
        }
        if let Some(target_index) = self
            .scene_configs
            .iter()
            .position(|scene| scene.scene_index == Some(target.index))
        {
            if self.scene_configs[target_index].internal_scene_id == source_internal_scene_id {
                return Ok(false);
            }
            if !overwrite_existing {
                return Err("Link blocked: target scene already has a config".to_string());
            }
            let removed_internal_scene_id = self.scene_configs[target_index].internal_scene_id;
            self.scene_configs.remove(target_index);
            if self.selected_scene_internal_id.as_deref()
                == Some(&removed_internal_scene_id.to_string())
            {
                self.selected_scene_internal_id = None;
            }
        }
        let source = self
            .get_scene_config_mut(source_internal_scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        source.scene_index = Some(target.index);
        source.scene_name = target.name.clone();
        Ok(true)
    }

    pub(crate) fn link_scene_config_by_index(
        &mut self,
        source_internal_scene_id: uuid::Uuid,
        target_scene_index: i32,
        overwrite_existing: bool,
    ) -> Result<bool, String> {
        let target = self
            .scene_list_entry_for_index(target_scene_index)
            .ok_or_else(|| "Link blocked: target scene not found".to_string())?;
        self.link_scene_config(source_internal_scene_id, &target, overwrite_existing)
    }

    pub(crate) fn delete_scene_config(
        &mut self,
        internal_scene_id: uuid::Uuid,
    ) -> Result<bool, String> {
        let Some(index) = self
            .scene_configs
            .iter()
            .position(|scene| scene.internal_scene_id == internal_scene_id)
        else {
            return Err("Scene config not found".to_string());
        };
        self.scene_configs.remove(index);
        if self.selected_scene_internal_id.as_deref() == Some(&internal_scene_id.to_string()) {
            self.selected_scene_internal_id = None;
        }
        Ok(true)
    }

    pub(crate) fn observe_scene_list(&mut self, scene_list: Vec<SceneListEntry>, now: Instant) {
        match self.last_scene_list.as_ref() {
            None => {
                self.last_scene_list = Some(scene_list);
            }
            Some(previous) if previous == &scene_list => {
                self.last_scene_list = Some(scene_list);
            }
            Some(_) => {
                self.last_scene_list = Some(scene_list);
                self.scene_list_edit_suppressed_until =
                    Some(now + SCENE_LIST_EDIT_SUPPRESSION_WINDOW);
            }
        }
    }

    pub(crate) fn observe_and_align_scene_list(
        &mut self,
        align_configs: bool,
        scene_list: Vec<SceneListEntry>,
        now: Instant,
    ) -> bool {
        let previous = self.scene_configs.clone();
        self.observe_scene_list(scene_list.clone(), now);
        if align_configs {
            self.scene_configs =
                align_scene_configs(std::mem::take(&mut self.scene_configs), &scene_list);
        }
        previous != self.scene_configs
    }

    pub(crate) fn is_scene_list_edit_suppressed(&self, now: Instant) -> bool {
        self.scene_list_edit_suppressed_until
            .map(|deadline| now < deadline)
            .unwrap_or(false)
    }

    pub(crate) fn accepts(&mut self, current_scene: &SceneState) -> bool {
        self.accepts_at(current_scene, Instant::now())
    }

    pub(crate) fn accepts_at(&mut self, current_scene: &SceneState, now: Instant) -> bool {
        let observed = ObservedScene {
            scene: RecallSceneIdentity::from(current_scene),
            at: now,
        };

        match &mut self.gate {
            RecallGate::Unarmed => {
                self.gate = RecallGate::Arming {
                    baseline: observed,
                    deadline: now + RECALL_ARMING_DELAY,
                };
                false
            }
            RecallGate::Arming { baseline, deadline } => {
                if now < *deadline {
                    *baseline = observed;
                    return false;
                }
                let baseline = baseline.clone();
                let mut last_trigger = None;
                let accepted = decide_armed(&baseline, &mut last_trigger, observed);
                self.gate = RecallGate::Armed {
                    baseline,
                    last_trigger,
                };
                accepted
            }
            RecallGate::Armed {
                baseline,
                last_trigger,
            } => decide_armed(baseline, last_trigger, observed),
        }
    }

    fn reset_recall_tracking(&mut self) {
        self.gate = RecallGate::default();
        self.scene_list_edit_suppressed_until = None;
    }
}

/// Armed-state decision: accept unless the observation repeats the last
/// trigger or echoes the arming baseline within `SAME_SCENE_REPEAT_DELAY`.
fn decide_armed(
    baseline: &ObservedScene,
    last_trigger: &mut Option<ObservedScene>,
    observed: ObservedScene,
) -> bool {
    if let Some(last) = last_trigger.as_ref()
        && last.scene == observed.scene
        && observed.at.duration_since(last.at) < SAME_SCENE_REPEAT_DELAY
    {
        return false;
    }

    if baseline.scene == observed.scene
        && observed.at.duration_since(baseline.at) < SAME_SCENE_REPEAT_DELAY
    {
        // Baseline echo: the pre-existing scene re-broadcast shortly after
        // arming. Record it as the last trigger so further echoes fall under
        // repeat suppression above, measured from the baseline observation.
        *last_trigger = Some(baseline.clone());
        return false;
    }

    *last_trigger = Some(observed);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::scenes::{ChannelConfig, ChannelRef, SceneScopeToggles};

    fn scene(index: i32, name: &str) -> SceneState {
        SceneState {
            index,
            name: name.to_string(),
        }
    }

    fn scene_entry(index: i32, name: &str) -> SceneListEntry {
        SceneListEntry {
            index,
            name: name.to_string(),
        }
    }

    fn scene_config(id: uuid::Uuid, seed: i32, linked: bool) -> SceneConfig {
        SceneConfig {
            internal_scene_id: id,
            scene_index: linked.then_some(seed),
            scene_name: format!("Scene {seed}"),
            duration_ms: (seed as u64) * 1_000,
            channel_configs: vec![ChannelConfig {
                group: seed,
                channel: seed + 1,
                fader_db: Some(-(seed as f64)),
                pan: Some(seed as f64 / 10.0),
                balance: Some(-(seed as f64) / 10.0),
                width: Some(seed as f64 / 5.0),
                pan_mode: Some(crate::lv1::PanMode::Stereo),
            }],
            scoped_channels: vec![ChannelRef {
                group: seed,
                channel: seed + 1,
            }],
            scope_toggles: SceneScopeToggles {
                faders: seed % 2 == 0,
                pan: seed % 2 != 0,
            },
        }
    }

    fn replace_scene_configs(state: &mut ScenesState, scene_configs: Vec<SceneConfig>) {
        state.replace_snapshot(SceneDocument {
            scene_configs,
            selected_scene_internal_id: None,
        });
    }

    fn assert_copied_settings(
        destination: &SceneConfig,
        source: &SceneConfig,
        destination_id: uuid::Uuid,
    ) {
        assert_eq!(destination.duration_ms, source.duration_ms);
        assert_eq!(destination.scope_toggles, source.scope_toggles);
        assert_eq!(destination.channel_configs, source.channel_configs);
        assert_eq!(destination.scoped_channels, source.scoped_channels);
        assert_eq!(destination.internal_scene_id, destination_id);
    }

    fn initial_scene_list() -> Vec<SceneListEntry> {
        vec![
            scene_entry(0, "My first scene"),
            scene_entry(1, "Song 1"),
            scene_entry(2, "My second scene"),
            scene_entry(3, "Song 2 -- Changed"),
            scene_entry(4, "Song 3"),
            scene_entry(5, "Test"),
        ]
    }

    fn moved_current_scene_list() -> Vec<SceneListEntry> {
        vec![
            scene_entry(0, "My first scene"),
            scene_entry(1, "Song 1"),
            scene_entry(2, "My second scene"),
            scene_entry(3, "Song 3"),
            scene_entry(4, "Song 2 -- Changed"),
            scene_entry(5, "Test"),
        ]
    }

    #[test]
    fn link_scene_config_uses_current_scene_list_entry() {
        let mut state = ScenesState::default();
        state.replace_snapshot(SceneDocument {
            scene_configs: vec![SceneConfig {
                internal_scene_id: uuid::Uuid::from_u128(1),
                scene_index: None,
                scene_name: "Source".to_string(),
                duration_ms: 1_000,
                channel_configs: vec![],
                scoped_channels: vec![],
                scope_toggles: Default::default(),
            }],
            selected_scene_internal_id: None,
        });
        state.observe_scene_list(vec![scene_entry(3, "Song 2 -- Changed")], Instant::now());

        assert!(
            state
                .link_scene_config_by_index(uuid::Uuid::from_u128(1), 3, false)
                .unwrap()
        );
        let linked = state.get_scene_config(uuid::Uuid::from_u128(1)).unwrap();
        assert_eq!(linked.scene_index, Some(3));
        assert_eq!(linked.scene_name, "Song 2 -- Changed");
    }

    #[test]
    fn link_scene_config_by_index_reuses_missing_target_error() {
        let mut state = ScenesState::default();
        state.replace_snapshot(SceneDocument {
            scene_configs: vec![SceneConfig {
                internal_scene_id: uuid::Uuid::from_u128(1),
                scene_index: None,
                scene_name: "Source".to_string(),
                duration_ms: 1_000,
                channel_configs: vec![],
                scoped_channels: vec![],
                scope_toggles: Default::default(),
            }],
            selected_scene_internal_id: None,
        });

        let err = state
            .link_scene_config_by_index(uuid::Uuid::from_u128(1), 99, false)
            .unwrap_err();
        assert_eq!(err, "Link blocked: target scene not found");
    }

    #[test]
    fn accepts_after_two_second_arming_delay() {
        let mut state = ScenesState::default();
        let scene = scene(1, "Intro");
        let start = Instant::now();

        assert!(!state.accepts_at(&scene, start));
        assert!(state.accepts_at(&scene, start + RECALL_ARMING_DELAY));
    }

    #[test]
    fn suppresses_same_scene_repeat_for_500ms() {
        let mut state = ScenesState::default();
        let scene = scene(1, "Intro");
        let start = Instant::now();

        assert!(!state.accepts_at(&scene, start));
        assert!(state.accepts_at(&scene, start + RECALL_ARMING_DELAY));
        assert!(!state.accepts_at(
            &scene,
            start + RECALL_ARMING_DELAY + SAME_SCENE_REPEAT_DELAY - Duration::from_millis(1)
        ));
        assert!(state.accepts_at(
            &scene,
            start + RECALL_ARMING_DELAY + SAME_SCENE_REPEAT_DELAY
        ));
    }

    #[test]
    fn baseline_scene_seen_shortly_after_arming_is_suppressed() {
        let mut state = ScenesState::default();
        let start = Instant::now();

        assert!(!state.accepts_at(&scene(1, "Intro"), start));
        // Scene re-observed late in the arming window becomes the baseline.
        assert!(!state.accepts_at(&scene(1, "Intro"), start + Duration::from_millis(1_900)));
        // The same scene re-broadcast just after arming is the pre-existing
        // scene, not an operator recall.
        assert!(!state.accepts_at(&scene(1, "Intro"), start + Duration::from_millis(2_100)));
    }

    #[test]
    fn suppressed_baseline_echo_counts_as_trigger_for_repeat_suppression() {
        let mut state = ScenesState::default();
        let start = Instant::now();

        assert!(!state.accepts_at(&scene(1, "Intro"), start));
        assert!(!state.accepts_at(&scene(1, "Intro"), start + Duration::from_millis(1_900)));
        assert!(!state.accepts_at(&scene(1, "Intro"), start + Duration::from_millis(2_100)));
        // Still within the repeat window measured from the baseline observation.
        assert!(!state.accepts_at(&scene(1, "Intro"), start + Duration::from_millis(2_350)));
        // Once the repeat window from the baseline observation has elapsed,
        // the same scene is a real recall again.
        assert!(state.accepts_at(&scene(1, "Intro"), start + Duration::from_millis(2_500)));
    }

    #[test]
    fn last_scene_seen_during_arming_becomes_the_suppressed_baseline() {
        let mut state = ScenesState::default();
        let start = Instant::now();

        assert!(!state.accepts_at(&scene(1, "Intro"), start));
        assert!(!state.accepts_at(&scene(2, "Verse"), start + Duration::from_millis(1_900)));
        // "Verse" is the baseline now, so "Intro" is a real scene change.
        assert!(state.accepts_at(&scene(1, "Intro"), start + Duration::from_millis(2_100)));
    }

    #[test]
    fn different_scene_right_after_arming_is_accepted() {
        let mut state = ScenesState::default();
        let start = Instant::now();

        assert!(!state.accepts_at(&scene(1, "Intro"), start));
        assert!(state.accepts_at(&scene(2, "Verse"), start + RECALL_ARMING_DELAY));
    }

    #[test]
    fn first_scene_list_establishes_baseline_without_suppression() {
        let mut state = ScenesState::default();
        let now = Instant::now();

        state.observe_scene_list(initial_scene_list(), now);

        assert!(!state.is_scene_list_edit_suppressed(now));
    }

    #[test]
    fn identical_scene_list_does_not_open_suppression_window() {
        let mut state = ScenesState::default();
        let now = Instant::now();

        state.observe_scene_list(initial_scene_list(), now);
        state.observe_scene_list(initial_scene_list(), now + Duration::from_millis(10));

        assert!(!state.is_scene_list_edit_suppressed(now + Duration::from_millis(10)));
    }

    #[test]
    fn changed_scene_list_suppresses_until_window_expires() {
        let mut state = ScenesState::default();
        let now = Instant::now();

        state.observe_scene_list(initial_scene_list(), now);
        state.observe_scene_list(moved_current_scene_list(), now + Duration::from_millis(10));

        assert!(state.is_scene_list_edit_suppressed(now + Duration::from_millis(10)));
        assert!(state.is_scene_list_edit_suppressed(now + Duration::from_millis(509)));
        assert!(!state.is_scene_list_edit_suppressed(now + Duration::from_millis(510)));
    }

    #[test]
    fn snapshot_round_trips_selected_scene_internal_id() {
        let mut state = ScenesState {
            selected_scene_internal_id: Some("scene-123".to_string()),
            ..Default::default()
        };

        let snapshot = state.snapshot();
        assert_eq!(
            snapshot.selected_scene_internal_id,
            Some("scene-123".to_string())
        );

        state.selected_scene_internal_id = None;
        state.replace_snapshot(snapshot);

        assert_eq!(
            state.selected_scene_internal_id,
            Some("scene-123".to_string())
        );
    }

    #[test]
    fn session_document_replacement_clears_recall_gate_and_edit_suppression() {
        let mut state = ScenesState::default();
        let now = Instant::now();

        assert!(!state.accepts_at(&scene(1, "Intro"), now));
        state.observe_scene_list(initial_scene_list(), now);
        state.observe_scene_list(moved_current_scene_list(), now + Duration::from_millis(10));
        assert!(state.is_scene_list_edit_suppressed(now + Duration::from_millis(10)));

        state.replace_snapshot_for_session(SceneDocument {
            scene_configs: vec![SceneConfig {
                internal_scene_id: uuid::Uuid::from_u128(1),
                scene_index: Some(1),
                scene_name: "Intro".to_string(),
                duration_ms: 1_000,
                channel_configs: vec![],
                scoped_channels: vec![],
                scope_toggles: Default::default(),
            }],
            selected_scene_internal_id: None,
        });

        assert!(!state.is_scene_list_edit_suppressed(now));
        assert!(!state.accepts_at(&scene(1, "Intro"), now + Duration::from_millis(1)));
    }

    #[test]
    fn copies_settings_from_a_linked_scene() {
        let source_id = uuid::Uuid::from_u128(1);
        let mut state = ScenesState::default();
        replace_scene_configs(&mut state, vec![scene_config(source_id, 1, true)]);

        assert_eq!(state.copy_scene_settings(source_id), Ok(true));
        assert!(state.projection_state().scene_settings_clipboard_available);
    }

    #[test]
    fn copies_settings_from_an_unlinked_scene() {
        let source_id = uuid::Uuid::from_u128(1);
        let mut state = ScenesState::default();
        replace_scene_configs(&mut state, vec![scene_config(source_id, 1, false)]);

        assert_eq!(state.copy_scene_settings(source_id), Ok(true));
        assert!(state.projection_state().scene_settings_clipboard_available);
    }

    #[test]
    fn copy_rejects_a_missing_source_scene() {
        let mut state = ScenesState::default();

        assert_eq!(
            state.copy_scene_settings(uuid::Uuid::from_u128(1)),
            Err("Scene config not found".to_string())
        );
        assert!(!state.projection_state().scene_settings_clipboard_available);
    }

    #[test]
    fn copied_settings_do_not_change_with_the_source_scene() {
        let source_id = uuid::Uuid::from_u128(1);
        let destination_id = uuid::Uuid::from_u128(2);
        let source = scene_config(source_id, 1, true);
        let mut state = ScenesState::default();
        replace_scene_configs(
            &mut state,
            vec![source.clone(), scene_config(destination_id, 2, true)],
        );

        assert_eq!(state.copy_scene_settings(source_id), Ok(true));
        state.get_scene_config_mut(source_id).unwrap().duration_ms = 9_999;

        assert_eq!(state.paste_scene_settings(destination_id), Ok(true));
        assert_copied_settings(
            &state.get_scene_config(destination_id).unwrap(),
            &source,
            destination_id,
        );
    }

    #[test]
    fn paste_rejects_a_missing_clipboard() {
        let destination_id = uuid::Uuid::from_u128(2);
        let mut state = ScenesState::default();
        replace_scene_configs(&mut state, vec![scene_config(destination_id, 2, true)]);

        assert_eq!(
            state.paste_scene_settings(destination_id),
            Err("Scene settings clipboard is empty".to_string())
        );
    }

    #[test]
    fn paste_rejects_a_missing_destination_scene() {
        let source_id = uuid::Uuid::from_u128(1);
        let mut state = ScenesState::default();
        replace_scene_configs(&mut state, vec![scene_config(source_id, 1, true)]);
        assert_eq!(state.copy_scene_settings(source_id), Ok(true));

        assert_eq!(
            state.paste_scene_settings(uuid::Uuid::from_u128(2)),
            Err("Scene config not found".to_string())
        );
    }

    #[test]
    fn paste_rejects_an_unlinked_destination_without_mutating_it() {
        let source_id = uuid::Uuid::from_u128(1);
        let destination_id = uuid::Uuid::from_u128(2);
        let source = scene_config(source_id, 1, true);
        let destination = scene_config(destination_id, 2, false);
        let mut state = ScenesState::default();
        replace_scene_configs(&mut state, vec![source.clone(), destination.clone()]);
        assert_eq!(state.copy_scene_settings(source_id), Ok(true));

        assert_eq!(
            state.paste_scene_settings(destination_id),
            Err("Paste blocked: destination scene is not linked".to_string())
        );
        let pasted = state.get_scene_config(destination_id).unwrap();
        assert_eq!(pasted, destination);
    }

    #[test]
    fn paste_changes_a_linked_destination_without_changing_its_identity() {
        let source_id = uuid::Uuid::from_u128(1);
        let destination_id = uuid::Uuid::from_u128(2);
        let source = scene_config(source_id, 1, true);
        let destination = scene_config(destination_id, 2, true);
        let mut state = ScenesState::default();
        replace_scene_configs(&mut state, vec![source.clone(), destination.clone()]);
        assert_eq!(state.copy_scene_settings(source_id), Ok(true));

        assert_eq!(state.paste_scene_settings(destination_id), Ok(true));
        let pasted = state.get_scene_config(destination_id).unwrap();
        assert_copied_settings(&pasted, &source, destination_id);
        assert_eq!(pasted.scene_index, destination.scene_index);
        assert_eq!(pasted.scene_name, destination.scene_name);
    }

    #[test]
    fn paste_is_a_no_op_when_destination_already_matches_the_clipboard() {
        let source_id = uuid::Uuid::from_u128(1);
        let destination_id = uuid::Uuid::from_u128(2);
        let source = scene_config(source_id, 1, true);
        let mut destination = source.clone();
        destination.internal_scene_id = destination_id;
        destination.scene_index = Some(2);
        destination.scene_name = "Destination".to_string();
        let mut state = ScenesState::default();
        replace_scene_configs(&mut state, vec![source, destination]);
        assert_eq!(state.copy_scene_settings(source_id), Ok(true));

        assert_eq!(state.paste_scene_settings(destination_id), Ok(false));
    }

    #[test]
    fn paste_leaves_the_source_scene_unchanged() {
        let source_id = uuid::Uuid::from_u128(1);
        let destination_id = uuid::Uuid::from_u128(2);
        let source = scene_config(source_id, 1, true);
        let mut state = ScenesState::default();
        replace_scene_configs(
            &mut state,
            vec![source.clone(), scene_config(destination_id, 2, true)],
        );
        assert_eq!(state.copy_scene_settings(source_id), Ok(true));

        assert_eq!(state.paste_scene_settings(destination_id), Ok(true));
        assert_eq!(state.get_scene_config(source_id), Some(source));
    }

    #[test]
    fn session_document_replacement_clears_the_settings_clipboard() {
        let source_id = uuid::Uuid::from_u128(1);
        let mut state = ScenesState::default();
        replace_scene_configs(&mut state, vec![scene_config(source_id, 1, true)]);
        assert_eq!(state.copy_scene_settings(source_id), Ok(true));

        state.replace_snapshot_for_session(SceneDocument::empty());

        assert!(!state.projection_state().scene_settings_clipboard_available);
    }
}

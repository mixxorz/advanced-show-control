use std::time::Duration;

use tokio::time::Instant;

use crate::lv1::{SceneListEntry, SceneState};
use crate::scenes::{SceneConfig, SceneDocument};

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
    pub(crate) scene_configs: Vec<SceneConfig>,
    pub(crate) cued_scene_internal_id: Option<uuid::Uuid>,
    pub(crate) selected_scene_internal_id: Option<String>,
    gate: RecallGate,
    last_scene_list: Option<Vec<SceneListEntry>>,
    scene_list_edit_suppressed_until: Option<Instant>,
}

impl ScenesState {
    pub fn set_lockout(&mut self, lockout: bool) {
        self.lockout = lockout;
    }

    pub fn lockout(&self) -> bool {
        self.lockout
    }

    pub fn projection_state(&self) -> crate::scenes::ScenesProjectionState {
        crate::scenes::ScenesProjectionState {
            scene_configs: self.scene_configs.clone(),
            cued_scene_internal_id: self.cued_scene_internal_id.map(|id| id.to_string()),
            selected_scene_internal_id: self.selected_scene_internal_id.clone(),
        }
    }

    pub fn snapshot(&self) -> SceneDocument {
        SceneDocument {
            scene_configs: self.scene_configs.clone(),
            cued_scene_internal_id: self.cued_scene_internal_id,
            selected_scene_internal_id: self.selected_scene_internal_id.clone(),
        }
    }

    pub fn replace_snapshot(&mut self, snapshot: SceneDocument) {
        self.scene_configs = snapshot.scene_configs;
        self.cued_scene_internal_id = snapshot.cued_scene_internal_id;
        self.selected_scene_internal_id = snapshot.selected_scene_internal_id;
        self.clear_missing_cue();
    }

    pub fn replace_snapshot_for_session(&mut self, snapshot: SceneDocument) {
        self.replace_snapshot(snapshot);
        self.reset_recall_tracking();
    }

    pub fn get_scene_config(&self, internal_scene_id: uuid::Uuid) -> Option<SceneConfig> {
        self.scene_configs
            .iter()
            .find(|scene| scene.internal_scene_id == internal_scene_id)
            .cloned()
    }

    pub fn scene_configs(&self) -> &[SceneConfig] {
        &self.scene_configs
    }

    pub fn scene_list_entry_for_index(&self, index: i32) -> Option<SceneListEntry> {
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

    pub fn cue_scene(&mut self, internal_scene_id: uuid::Uuid) -> Result<bool, String> {
        if self.get_scene_config(internal_scene_id).is_none() {
            return Err("Scene config not found".to_string());
        }
        let next = Some(internal_scene_id);
        if self.cued_scene_internal_id == next {
            return Ok(false);
        }
        self.cued_scene_internal_id = next;
        Ok(true)
    }

    pub fn select_scene_config(&mut self, internal_scene_id: uuid::Uuid) -> Result<bool, String> {
        if self.get_scene_config(internal_scene_id).is_none() {
            return Err("Scene config not found".to_string());
        }
        let next = Some(internal_scene_id.to_string());
        if self.selected_scene_internal_id == next {
            return Ok(false);
        }
        self.selected_scene_internal_id = next;
        if self.cued_scene_internal_id == Some(internal_scene_id) {
            self.cued_scene_internal_id = None;
        }
        Ok(true)
    }

    pub fn link_scene_config(
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
            if self.cued_scene_internal_id == Some(removed_internal_scene_id) {
                self.cued_scene_internal_id = None;
            }
        }
        let source = self
            .get_scene_config_mut(source_internal_scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        source.scene_index = Some(target.index);
        source.scene_name = target.name.clone();
        Ok(true)
    }

    pub fn link_scene_config_by_index(
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

    pub fn delete_scene_config(&mut self, internal_scene_id: uuid::Uuid) -> Result<bool, String> {
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
        if self.cued_scene_internal_id == Some(internal_scene_id) {
            self.cued_scene_internal_id = None;
        }
        Ok(true)
    }
    pub fn observe_scene_list(&mut self, scene_list: Vec<SceneListEntry>, now: Instant) {
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

    pub fn is_scene_list_edit_suppressed(&self, now: Instant) -> bool {
        self.scene_list_edit_suppressed_until
            .map(|deadline| now < deadline)
            .unwrap_or(false)
    }

    pub fn accepts(&mut self, current_scene: &SceneState) -> bool {
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
            cued_scene_internal_id: None,
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
            cued_scene_internal_id: None,
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
            cued_scene_internal_id: None,
            selected_scene_internal_id: None,
        });

        assert!(!state.is_scene_list_edit_suppressed(now));
        assert!(!state.accepts_at(&scene(1, "Intro"), now + Duration::from_millis(1)));
    }
}

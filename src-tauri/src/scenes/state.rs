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
    pub(crate) scene_configs: Vec<SceneConfig>,
    pub(crate) cued_scene_internal_id: Option<uuid::Uuid>,
    gate: RecallGate,
    last_scene_list: Option<Vec<SceneListEntry>>,
    scene_list_edit_suppressed_until: Option<Instant>,
}

impl ScenesState {
    pub fn snapshot(&self) -> SceneDocument {
        SceneDocument {
            scene_configs: self.scene_configs.clone(),
            cued_scene_internal_id: self.cued_scene_internal_id,
        }
    }

    pub fn replace_snapshot(&mut self, snapshot: SceneDocument) {
        self.scene_configs = snapshot.scene_configs;
        self.cued_scene_internal_id = snapshot.cued_scene_internal_id;
        self.clear_missing_cue();
    }

    pub fn get_scene_config(&self, internal_scene_id: uuid::Uuid) -> Option<SceneConfig> {
        self.scene_configs
            .iter()
            .find(|scene| scene.internal_scene_id == internal_scene_id)
            .cloned()
    }

    pub(crate) fn get_scene_config_mut(
        &mut self,
        internal_scene_id: uuid::Uuid,
    ) -> Option<&mut SceneConfig> {
        self.scene_configs
            .iter_mut()
            .find(|scene| scene.internal_scene_id == internal_scene_id)
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
}

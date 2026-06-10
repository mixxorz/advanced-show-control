use std::time::Duration;

use tokio::time::Instant;

use crate::lv1::types::{SceneListEntry, SceneState};

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

#[derive(Debug, Default)]
pub struct SceneRecallState {
    baseline: Option<RecallSceneIdentity>,
    baseline_at: Option<Instant>,
    arm_after: Option<Instant>,
    last_scene: Option<RecallSceneIdentity>,
    last_triggered_at: Option<Instant>,
    last_scene_list: Option<Vec<SceneListEntry>>,
    scene_list_edit_suppressed_until: Option<Instant>,
}

impl SceneRecallState {
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
        let scene_identity = RecallSceneIdentity::from(current_scene);

        match self.arm_after {
            None => {
                self.baseline = Some(scene_identity);
                self.baseline_at = Some(now);
                self.arm_after = Some(now + RECALL_ARMING_DELAY);
                false
            }
            Some(deadline) if now < deadline => {
                self.baseline = Some(scene_identity);
                self.baseline_at = Some(now);
                false
            }
            Some(_) => {
                if self.last_scene.as_ref() == Some(&scene_identity)
                    && self
                        .last_triggered_at
                        .map(|triggered_at| {
                            now.duration_since(triggered_at) < SAME_SCENE_REPEAT_DELAY
                        })
                        .unwrap_or(false)
                {
                    return false;
                }

                let baseline_scene = self
                    .baseline
                    .clone()
                    .unwrap_or_else(|| scene_identity.clone());
                let triggered_at = self.baseline_at.unwrap_or(now);
                self.last_scene = Some(baseline_scene);
                self.last_triggered_at = Some(triggered_at);

                if self.last_scene.as_ref() == Some(&scene_identity)
                    && now.duration_since(triggered_at) < SAME_SCENE_REPEAT_DELAY
                {
                    return false;
                }

                self.last_scene = Some(scene_identity);
                self.last_triggered_at = Some(now);
                true
            }
        }
    }
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
        let mut state = SceneRecallState::default();
        let scene = scene(1, "Intro");
        let start = Instant::now();

        assert!(!state.accepts_at(&scene, start));
        assert!(state.accepts_at(&scene, start + RECALL_ARMING_DELAY));
    }

    #[test]
    fn suppresses_same_scene_repeat_for_500ms() {
        let mut state = SceneRecallState::default();
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
    fn first_scene_list_establishes_baseline_without_suppression() {
        let mut state = SceneRecallState::default();
        let now = Instant::now();

        state.observe_scene_list(initial_scene_list(), now);

        assert!(!state.is_scene_list_edit_suppressed(now));
    }

    #[test]
    fn identical_scene_list_does_not_open_suppression_window() {
        let mut state = SceneRecallState::default();
        let now = Instant::now();

        state.observe_scene_list(initial_scene_list(), now);
        state.observe_scene_list(initial_scene_list(), now + Duration::from_millis(10));

        assert!(!state.is_scene_list_edit_suppressed(now + Duration::from_millis(10)));
    }

    #[test]
    fn changed_scene_list_suppresses_until_window_expires() {
        let mut state = SceneRecallState::default();
        let now = Instant::now();

        state.observe_scene_list(initial_scene_list(), now);
        state.observe_scene_list(moved_current_scene_list(), now + Duration::from_millis(10));

        assert!(state.is_scene_list_edit_suppressed(now + Duration::from_millis(10)));
        assert!(state.is_scene_list_edit_suppressed(now + Duration::from_millis(509)));
        assert!(!state.is_scene_list_edit_suppressed(now + Duration::from_millis(510)));
    }
}

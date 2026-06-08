use std::collections::HashSet;
use std::time::Duration;

use tokio::time::Instant;

use crate::lv1::types::SceneState;

const RECALL_ARMING_DELAY: Duration = Duration::from_millis(2_000);
const SAME_SCENE_REPEAT_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecallSceneIdentity {
    index: i32,
    name: String,
}

impl From<&SceneState> for RecallSceneIdentity {
    fn from(scene: &SceneState) -> Self {
        Self { index: scene.index, name: scene.name.clone() }
    }
}

#[derive(Debug, Default)]
pub struct SceneRecallState {
    baseline: Option<RecallSceneIdentity>,
    baseline_at: Option<Instant>,
    arm_after: Option<Instant>,
    last_scene: Option<RecallSceneIdentity>,
    last_triggered_at: Option<Instant>,
    duration_zero_skip_scene_ids: HashSet<String>,
}

impl SceneRecallState {
    pub fn reset_for_generation(&mut self) {
        *self = Self::default();
    }

    pub fn accepts(&mut self, current_scene: &SceneState) -> bool {
        let now = Instant::now();
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
                        .map(|triggered_at| now.duration_since(triggered_at) < SAME_SCENE_REPEAT_DELAY)
                        .unwrap_or(false)
                {
                    return false;
                }

                let baseline_scene = self.baseline.clone().unwrap_or_else(|| scene_identity.clone());
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

    pub fn should_log_duration_zero_skip(&mut self, scene_id: &str) -> bool {
        self.duration_zero_skip_scene_ids.insert(scene_id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(index: i32, name: &str) -> SceneState {
        SceneState { index, name: name.to_string() }
    }

    #[test]
    fn accepts_after_two_second_arming_delay() {
        let mut state = SceneRecallState::default();
        let scene = scene(1, "Intro");

        assert!(!state.accepts(&scene));
        std::thread::sleep(Duration::from_secs(2));

        assert!(state.accepts(&scene));
    }

    #[test]
    fn suppresses_same_scene_repeat_for_500ms() {
        let mut state = SceneRecallState::default();
        let scene = scene(1, "Intro");

        assert!(!state.accepts(&scene));
        std::thread::sleep(Duration::from_secs(2));
        assert!(state.accepts(&scene));
        assert!(!state.accepts(&scene));
        std::thread::sleep(Duration::from_millis(500));
        assert!(state.accepts(&scene));
    }

    #[test]
    fn duration_zero_skip_logs_once_until_reset() {
        let mut state = SceneRecallState::default();

        assert!(state.should_log_duration_zero_skip("1::Intro"));
        assert!(!state.should_log_duration_zero_skip("1::Intro"));

        state.reset_for_generation();

        assert!(state.should_log_duration_zero_skip("1::Intro"));
    }
}

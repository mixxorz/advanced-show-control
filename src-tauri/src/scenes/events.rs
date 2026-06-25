use crate::scenes::SceneConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenesProjectionReason {
    SceneState,
    FileReplacement,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenesProjectionState {
    pub scene_configs: Vec<SceneConfig>,
    pub cued_scene_internal_id: Option<String>,
    pub selected_scene_internal_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScenesEvent {
    StateChanged {
        reason: ScenesProjectionReason,
        state: ScenesProjectionState,
        persisted_scene_edit: bool,
    },
    Skipped {
        scene_label: String,
        reason: String,
    },
    Blocked {
        scene_label: String,
        reason: String,
    },
    Ready {
        scene_label: String,
        target_count: usize,
    },
    StartRequested {
        scene_label: String,
    },
}

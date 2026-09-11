use crate::scenes::SceneConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct ScenesProjectionState {
    pub scene_configs: Vec<SceneConfig>,
    pub selected_scene_internal_id: Option<String>,
    pub scene_settings_clipboard_available: bool,
    /// The runtime generation whose LV1 scene library reconciled these configurations.
    pub ready_generation: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScenesEvent {
    StateChanged {
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

mod actor;
mod capture;
mod commands;
mod events;
mod policy;
mod recall_queue;
mod scene_alignment;
mod state;
mod types;

pub use actor::{ScenesPeers, ScenesTask, build_scenes_actor};
pub use commands::{
    RecallSceneResult, ScenesCommand, ScenesCommandResult, SelectedSceneResult,
    validate_recall_scene_request,
};
pub use events::{ScenesEvent, ScenesProjectionState};
pub type ScenesHandle = tokio::sync::mpsc::Sender<ScenesCommand>;
pub(crate) use scene_alignment::{align_scene_configs, scene_alignment_diagnostic};
pub(crate) use state::ScenesState;
pub use types::{ChannelConfig, ChannelRef, SceneConfig, SceneDocument, SceneScopeToggles};

pub(crate) fn is_supported_scope_group(group: i32) -> bool {
    matches!(group, 0..=8 | 12)
}

#[cfg(test)]
mod tests {
    use super::is_supported_scope_group;

    #[test]
    fn supported_scope_groups_match_the_visible_lv1_channel_families() {
        for group in 0..=8 {
            assert!(is_supported_scope_group(group));
        }
        assert!(is_supported_scope_group(12));
        assert!(!is_supported_scope_group(9));
        assert!(!is_supported_scope_group(24));
    }
}

mod actor;
mod capture;
mod commands;
mod events;
mod handle;
mod policy;
mod scene_alignment;
mod state;
mod types;

pub use actor::{ScenesPeers, ScenesTask, build_scenes_actor};
pub use commands::{
    CueSceneResult, RecallSceneResult, ScenesCommand, ScenesCommandResult, SelectedSceneResult,
    validate_recall_scene_request,
};
pub use events::{ScenesEvent, ScenesProjectionReason, ScenesProjectionState};
pub use handle::ScenesHandle;
pub(crate) use scene_alignment::{align_scene_configs, scene_alignment_diagnostic};
pub(crate) use state::ScenesState;
pub use types::{ChannelConfig, ChannelRef, SceneConfig, SceneDocument, SceneScopeToggles};

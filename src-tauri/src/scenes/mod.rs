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

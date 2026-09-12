mod actor;
mod commands;
mod curve;
mod events;
mod fader_law;
mod state;
mod tick;
mod types;

pub use actor::{FadeEngineTask, build_engine};
pub use commands::{
    FadeCommand, RecallReadinessCancellation, RecallReadinessError, RecallReadinessRequest,
    SameSceneRecallBehavior,
};
pub use curve::FadeCurve;
pub use events::FadeEvent;
pub use fader_law::pos_to_db;
pub type FadeEngineHandle = tokio::sync::mpsc::Sender<FadeCommand>;
pub use types::{FadeConfig, FadeParameter, FadeSceneIdentity, FadeTarget, FadeTargetKey};

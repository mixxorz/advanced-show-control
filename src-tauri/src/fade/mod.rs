mod actor;
mod commands;
mod curve;
mod events;
mod fader_law;
mod handle;
mod state;
mod tick;
mod types;

pub use actor::spawn_engine;
#[cfg(test)]
pub(crate) use commands::FadeCommand;
pub use curve::FadeCurve;
pub use events::FadeEvent;
pub use fader_law::pos_to_db;
pub use handle::FadeEngineHandle;
pub use types::{FadeConfig, FadeParameter, FadeSceneIdentity, FadeTarget, FadeTargetKey};

//! AppViewState projection for the frontend status channel.
//!
//! The projector is the backend owner of the `app-status-changed` event.

mod cache;
mod runtime;
mod view;

pub use cache::{MAX_PROJECTOR_LOGS, ProjectionCache};
pub use runtime::{PROJECTOR_INTERVAL, ProjectorInputs, spawn_projector};
pub use view::{
    AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, LogSeverity,
    SceneSummary,
};

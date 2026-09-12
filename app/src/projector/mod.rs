//! AppViewState projection for native presentation state.
//!
//! The projector owns the latest-value state stream consumed by native presentation hosts.

mod cache;
mod runtime;
mod view;

pub use cache::{MAX_PROJECTOR_LOGS, ProjectionCache};
pub use runtime::{
    PROJECTOR_INTERVAL, ProjectionSink, ProjectionSubscription, ProjectorInputs,
    projection_channel, spawn_projector,
};
pub use view::{
    AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, LogSeverity,
    SceneSummary,
};

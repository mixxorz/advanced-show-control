//! AppViewState projection and `app-status-changed` emission.
//!
//! The projector will become the only backend owner of app-status-changed emission.

mod cache;
mod runtime;

pub use cache::{MAX_PROJECTOR_LOGS, ProjectionCache};
pub use runtime::{PROJECTOR_INTERVAL, ProjectorInputs, spawn_projector};

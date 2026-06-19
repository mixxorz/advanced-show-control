//! AppViewState projection and `app-status-changed` emission.
//!
//! The projector will become the only backend owner of app-status-changed emission.

mod cache;

pub use cache::{MAX_PROJECTOR_LOGS, ProjectionCache};

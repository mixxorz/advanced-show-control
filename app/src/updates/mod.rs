//! App-lifetime updater owner. Stable uses GitHub's latest-release HTTP assets, avoiding the
//! ten-release GitHub listing limit. Nightly selects the nightly channel (not a merged feed).
//! Turning nightly off waits for a newer stable release; installed versions are never downgraded.
mod actor;
mod backend;
#[cfg(test)]
mod tests;

pub use actor::{UpdatesTask, build_updates_actor};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

#[derive(Default, Clone, Debug, PartialEq, Serialize)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Ready,
    Unavailable,
    Error,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize)]
pub struct UpdateState {
    pub status: UpdateStatus,
    pub available_version: Option<String>,
    pub error: Option<String>,
    pub current_version: Option<String>,
}

pub type UpdatesHandle = mpsc::Sender<UpdatesCommand>;
#[derive(Debug)]
pub enum UpdatesCommand {
    Check {
        reply: oneshot::Sender<Result<(), String>>,
    },
    Download {
        reply: oneshot::Sender<Result<(), String>>,
    },
    PrepareInstall {
        reply: oneshot::Sender<Result<UpdateInstallPlan, String>>,
    },
}

/// @cc [owner:mixxorz,label:safety] deferred-update-install
/// Creating or preparing a plan MUST NOT launch an installer. Launch MUST wait for process exit,
/// remain non-silent, and restart the application without forwarding process arguments.
/// Preparation MUST retain the staged plan so dropping a prepared copy permits a retry.
#[derive(Clone)]
pub struct UpdateInstallPlan {
    launch: Arc<dyn Fn() -> Result<(), String> + Send + Sync>,
    authority: Option<Arc<PlanAuthority>>,
}
struct PlanAuthority {
    settings: tokio::sync::watch::Receiver<crate::runtime::AppStateSnapshot>,
    expected_revision: u64,
}
impl std::fmt::Debug for UpdateInstallPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateInstallPlan").finish_non_exhaustive()
    }
}
impl UpdateInstallPlan {
    /// @cc [owner:mixxorz,label:safety] issued-update-plan-authority
    /// Launch MUST reject a plan whose retained channel-policy revision differs from issuance,
    /// including coalesced channel toggles returning to the original policy. All copies MUST
    /// remain invalid after any channel-policy change.
    pub fn launch_after_exit(self) -> Result<(), String> {
        self.validate()?;
        (self.launch)()
    }

    fn validate(&self) -> Result<(), String> {
        let valid = self.authority.as_ref().is_some_and(|authority| {
            authority.settings.borrow().update_channel_revision == authority.expected_revision
        });
        if valid {
            Ok(())
        } else {
            Err("The update channel changed; please prepare the update again.".into())
        }
    }

    fn bind(mut self, bus: &crate::runtime::events::AppEventBus, expected_revision: u64) -> Self {
        self.authority = Some(Arc::new(PlanAuthority {
            settings: bus.state(),
            expected_revision,
        }));
        self
    }
}

// One bounded in-memory installation intent shared with the native shutdown guard.
// Hosts arm only after successful asynchronous disconnect, inside final dirty-session admission.
// The native quit observer consumes the plan before AppKit exits; event-loop return is not
// guaranteed on macOS. Dropping an unconsumed queue never installs an update.
/// @cc [owner:mixxorz,label:safety;concurrency] bounded-install-admission
/// Arm and take MUST only replace or consume one in-memory plan under a synchronous mutex.
/// Arm MUST synchronously validate backend channel authority before storing a plan.
/// They MUST NOT perform I/O, await an actor, or launch installation.
#[derive(Clone, Default)]
pub struct UpdateInstallQueue(Arc<Mutex<Option<UpdateInstallPlan>>>);
impl UpdateInstallQueue {
    pub fn arm(&self, plan: UpdateInstallPlan) -> Result<(), String> {
        plan.validate()?;
        *self.0.lock().expect("update install queue poisoned") = Some(plan);
        Ok(())
    }
    pub fn take(&self) -> Option<UpdateInstallPlan> {
        self.0.lock().expect("update install queue poisoned").take()
    }
}

#[derive(Debug, PartialEq)]
enum ReleaseSource {
    StableHttp,
    NightlyGithub,
}
struct ReleaseRoute {
    channel: String,
    source: ReleaseSource,
}
fn release_route(nightly: bool, platform: &str) -> ReleaseRoute {
    ReleaseRoute {
        channel: format!("{platform}-{}", if nightly { "nightly" } else { "stable" }),
        source: if nightly {
            ReleaseSource::NightlyGithub
        } else {
            ReleaseSource::StableHttp
        },
    }
}

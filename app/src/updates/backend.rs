use super::{ReleaseSource, UpdateInstallPlan, release_route};
use std::sync::Arc;
use velopack::sources::{GithubSource, HttpSource};
use velopack::{UpdateCheck, UpdateInfo, UpdateManager, UpdateOptions};

pub(super) enum BackendError {
    Unavailable(String),
    Failed(String),
    InstalledFailure { reason: String, current: String },
}
pub(super) struct Checked {
    pub current: String,
    pub candidate: Option<Arc<dyn Candidate>>,
}
pub(super) trait Backend: Send + Sync {
    fn check(&self, nightly: bool, network: bool) -> Result<Checked, BackendError>;
}
pub(super) trait Candidate: Send + Sync {
    fn version(&self) -> String;
    fn download(&self) -> Result<UpdateInstallPlan, String>;
}
pub(super) struct VelopackBackend;
struct VelopackCandidate {
    manager: UpdateManager,
    info: UpdateInfo,
}

/// @cc [owner:mixxorz,label:safety;updates] updater-source-and-install-boundary
/// Stable checks MUST use latest-release HTTP assets, nightly checks MUST use prerelease GitHub
/// assets, and both MUST specify the platform channel with downgrade disabled. An unpackaged app
/// MUST return Unavailable before network I/O. Downloads MUST use Velopack's size/hash verification
/// and MUST NOT launch installation.
impl Backend for VelopackBackend {
    fn check(&self, nightly: bool, network: bool) -> Result<Checked, BackendError> {
        let platform = if cfg!(target_os = "windows") {
            "win-x64"
        } else if cfg!(target_os = "macos") {
            "osx-universal"
        } else {
            return Err(BackendError::Unavailable(
                "Updates are unavailable on this platform.".into(),
            ));
        };
        let route = release_route(nightly, platform);
        let options = Some(UpdateOptions {
            ExplicitChannel: Some(route.channel),
            AllowVersionDowngrade: false,
            ..Default::default()
        });
        let manager = match route.source {
            ReleaseSource::StableHttp => UpdateManager::new(HttpSource::new_with_options("https://github.com/mixxorz/advanced-show-control/releases/latest/download", velopack::HttpOptions {
                // This source shares one timeout for feeds and potentially large package downloads.
                TimeoutMilliseconds: 300_000,
                ..Default::default()
            }), options, None),
            ReleaseSource::NightlyGithub => UpdateManager::new(GithubSource::new("https://github.com/mixxorz/advanced-show-control", None, true), options, None),
        }.map_err(|error| match error {
            velopack::Error::NotInstalled(_) => BackendError::Unavailable("Updates are unavailable in this copy of Advanced Show Control. Install the latest release from GitHub to enable updates.".into()),
            _ => BackendError::Failed(format!("Could not initialize updates: {error}")),
        })?;
        let current = manager.get_current_version_as_string();
        let candidate = if network {
            match manager
                .check_for_updates()
                .map_err(|error| BackendError::InstalledFailure {
                    reason: format!("Could not check for updates: {error}"),
                    current: current.clone(),
                })? {
                UpdateCheck::UpdateAvailable(info) => Some(Arc::new(VelopackCandidate {
                    manager,
                    info: *info,
                })
                    as Arc<dyn Candidate>),
                UpdateCheck::NoUpdateAvailable => None,
                UpdateCheck::RemoteIsEmpty => {
                    return Err(BackendError::InstalledFailure {
                        reason: "The update channel has no release packages available.".into(),
                        current,
                    });
                }
            }
        } else {
            None
        };
        Ok(Checked { current, candidate })
    }
}
impl Candidate for VelopackCandidate {
    fn version(&self) -> String {
        self.info.TargetFullRelease.Version.clone()
    }
    fn download(&self) -> Result<UpdateInstallPlan, String> {
        self.manager
            .download_updates(&self.info, None)
            .map_err(|error| format!("Could not download the update: {error}"))?;
        let manager = self.manager.clone();
        let info = self.info.clone();
        Ok(UpdateInstallPlan {
            authority: None,
            launch: Arc::new(move || {
                manager
                    .wait_exit_then_apply_updates(info.clone(), false, true, Vec::<String>::new())
                    .map_err(|error| format!("Could not launch the update installer: {error}"))
            }),
        })
    }
}

use super::backend::{Backend, BackendError, Candidate, Checked, VelopackBackend};
use super::{UpdateChannel, UpdateInstallPlan, UpdateState, UpdateStatus, UpdatesCommand, UpdatesHandle};
use crate::runtime::events::{AppEvent, AppEventBus};
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot};

type Reply = oneshot::Sender<Result<(), String>>;
enum Completion {
    Check(Result<Checked, BackendError>, bool),
    Download(Result<UpdateInstallPlan, String>),
}
struct Pending {
    task: tokio::task::JoinHandle<Completion>,
    channel: UpdateChannel,
    reply: Option<Reply>,
}
// The published status is presentation; these states own the actual capabilities.
enum ReleaseCapability {
    Empty,
    Available(Arc<dyn Candidate>),
    Downloaded { candidate: Option<Arc<dyn Candidate>>, plan: UpdateInstallPlan },
}
impl ReleaseCapability {
    fn candidate(&self) -> Option<Arc<dyn Candidate>> {
        match self { Self::Empty => None, Self::Available(candidate) => Some(candidate.clone()), Self::Downloaded { candidate, .. } => candidate.clone() }
    }
    fn plan(&self) -> Option<&UpdateInstallPlan> {
        match self { Self::Downloaded { plan, .. } => Some(plan), _ => None }
    }
    fn checked(candidate: Option<Arc<dyn Candidate>>) -> Self {
        match candidate { Some(candidate) => Self::Available(candidate), None => Self::Empty }
    }
}
enum UpdateWorker {
    Idle,
    Checking(Pending),
    Downloading(Pending),
}
impl UpdateWorker {
    fn running(&self) -> bool { !matches!(self, Self::Idle) }
    fn operation_mut(&mut self) -> Option<&mut Pending> {
        match self { Self::Idle => None, Self::Checking(operation) | Self::Downloading(operation) => Some(operation) }
    }
    fn complete(&mut self) -> Pending {
        match std::mem::replace(self, Self::Idle) {
            Self::Checking(operation) | Self::Downloading(operation) => operation,
            Self::Idle => unreachable!("only a running worker completes"),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum AutomaticCheck { None, Queued }
impl From<bool> for AutomaticCheck {
    fn from(enabled: bool) -> Self { if enabled { Self::Queued } else { Self::None } }
}

pub struct UpdatesTask {
    bus: AppEventBus,
    commands: mpsc::Receiver<UpdatesCommand>,
    backend: Arc<dyn Backend>,
}
pub fn build_updates_actor(event_bus: AppEventBus) -> (UpdatesHandle, UpdatesTask) {
    build(event_bus, Arc::new(VelopackBackend))
}
pub(super) fn build(bus: AppEventBus, backend: Arc<dyn Backend>) -> (UpdatesHandle, UpdatesTask) {
    let (handle, commands) = mpsc::channel(16);
    (
        handle,
        UpdatesTask {
            bus,
            commands,
            backend,
        },
    )
}
impl UpdatesTask {
    pub fn spawn(self) {
        tokio::spawn(self.run());
    }

    /**
     * @cc [owner:mixxorz,label:safety;concurrency] update-channel-invalidation
     * A different current update channel MUST clear available and downloaded intent and reject
     * pending replies. Blocking completions from a different current channel MUST NOT publish
     * state, log success, or supply installation plans. Returning to the operation's channel
     * MAY accept its completion; intervening toggles do not permanently invalidate it.
     * Settings MUST remain responsive during blocking I/O.
     */
    /**
     * @cc [owner:mixxorz,label:safety;updates] explicit-download-and-install-intent
     * Automatic checks MUST honor the latest retained setting. Check and Download MUST never
     * install, and only explicit Download may download.
     */
    async fn run(mut self) {
        let mut retained = self.bus.state();
        let settings = retained.borrow_and_update().settings.clone();
        let mut channel = UpdateChannel::from(settings.include_nightly_updates);
        let mut automatic = settings.automatically_check_for_updates;
        let mut state = UpdateState::default();
        let mut capability = ReleaseCapability::Empty;
        let mut worker = UpdateWorker::Checking(self.start_check(channel, automatic, None));
        let mut automatic_check = AutomaticCheck::None;
        let mut timer = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(6 * 3600),
            Duration::from_secs(6 * 3600),
        );
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        if automatic {
            state.status = UpdateStatus::Checking;
        }
        self.publish(&state);
        loop {
            tokio::select! {
                changed = retained.changed() => {
                    if changed.is_err() { break; }
                    let settings = retained.borrow_and_update().settings.clone();
                    let channel_changed = channel != UpdateChannel::from(settings.include_nightly_updates);
                    let newly_enabled = !automatic && settings.automatically_check_for_updates;
                    automatic = settings.automatically_check_for_updates;
                    if !automatic { automatic_check = AutomaticCheck::None; }
                    if channel_changed {
                        channel = UpdateChannel::from(settings.include_nightly_updates);
                        capability = ReleaseCapability::Empty;
                        if let Some(operation) = worker.operation_mut()
                            && let Some(reply) = operation.reply.take()
                        {
                            let _ = reply.send(Err("The update channel changed; please try again.".into()));
                        }
                        if state.status != UpdateStatus::Unavailable {
                            state.status = UpdateStatus::Idle;
                            state.error = None;
                        }
                        state.available_version = None;
                        self.publish(&state);
                        automatic_check = automatic.into();
                    } else if newly_enabled { automatic_check = AutomaticCheck::Queued; }
                }
                _ = timer.tick() => { automatic_check = automatic.into(); }
                command = self.commands.recv() => {
                    let Some(command) = command else { break; };
                    match command {
                        UpdatesCommand::Check { reply } => {
                            if worker.running() {
                                let _ = reply.send(Err("An update operation is already in progress.".into()));
                                continue;
                            }
                            capability = ReleaseCapability::Empty;
                            state.available_version = None;
                            state.error = None;
                            state.status = UpdateStatus::Checking;
                            self.publish(&state);
                            worker = UpdateWorker::Checking(self.start_check(channel, true, Some(reply)));
                        }
                        UpdatesCommand::Download { reply } => {
                            if worker.running() {
                                let _ = reply.send(Err("An update operation is already in progress.".into()));
                                continue;
                            }
                            let Some(update) = capability.candidate() else {
                                let _ = reply.send(Err("No update is available to download.".into()));
                                continue;
                            };
                            capability = ReleaseCapability::Available(update.clone());
                            state.status = UpdateStatus::Downloading;
                            state.error = None;
                            self.publish(&state);
                            worker = UpdateWorker::Downloading(Pending {
                                task: tokio::task::spawn_blocking(move || Completion::Download(update.download())),
                                channel,
                                reply: Some(reply),
                            });
                        }
                        UpdatesCommand::PrepareInstall { reply } => {
                            // Read retained policy as well as watch notifications before handing out intent.
                            if UpdateChannel::from(retained.borrow().settings.include_nightly_updates) != channel {
                                let _ = reply.send(Err("The update channel changed; please try again.".into()));
                            } else {
                                let result = capability.plan().cloned().ok_or_else(|| "No downloaded update is ready to install.".into());
                                let _ = reply.send(result);
                            }
                        }
                    }
                }
                result = async { (&mut worker.operation_mut().expect("pending operation").task).await }, if worker.running() => {
                    let operation = worker.complete();
                    // Retained settings are authoritative even if watch delivery loses the select race.
                    if UpdateChannel::from(retained.borrow().settings.include_nightly_updates) != operation.channel {
                        if let Some(reply) = operation.reply { let _ = reply.send(Err("The update channel changed; please try again.".into())); }
                        if automatic_check == AutomaticCheck::Queued && automatic
                            && retained.borrow().settings.automatically_check_for_updates
                            && UpdateChannel::from(retained.borrow().settings.include_nightly_updates) == channel
                        {
                            automatic_check = AutomaticCheck::None;
                            state.status = UpdateStatus::Checking;
                            self.publish(&state);
                            worker = UpdateWorker::Checking(self.start_check(channel, true, None));
                        }
                        continue;
                    }
                    channel = operation.channel;
                    let downloaded = matches!(&result, Ok(Completion::Download(Ok(_))));
                    let outcome = match result {
                        Ok(Completion::Check(Ok(checked), network)) => {
                            state.current_version = Some(checked.current);
                            let same_downloaded_version = capability.plan().is_some()
                                && capability.candidate().map(|update| update.version())
                                    == checked.candidate.as_ref().map(|update| update.version());
                            if !same_downloaded_version {
                                capability = ReleaseCapability::checked(checked.candidate);
                            }
                            state.available_version = capability.candidate().map(|update| update.version());
                            state.status = if same_downloaded_version {
                                UpdateStatus::Ready
                            } else if capability.candidate().is_some() {
                                UpdateStatus::Available
                            } else if network {
                                UpdateStatus::UpToDate
                            } else {
                                UpdateStatus::Idle
                            };
                            state.error = None;
                            Ok(())
                        }
                        Ok(Completion::Check(Err(BackendError::Unavailable(reason)), _)) => {
                            state.status = UpdateStatus::Unavailable;
                            state.error = Some(reason.clone());
                            Err(reason)
                        }
                        Ok(Completion::Check(Err(BackendError::InstalledFailure { reason, current }), _)) => {
                            state.current_version = Some(current);
                            state.status = UpdateStatus::Error;
                            state.error = Some(reason.clone());
                            Err(reason)
                        }
                        Ok(Completion::Check(Err(BackendError::Failed(reason)), _)) | Ok(Completion::Download(Err(reason))) => {
                            state.status = UpdateStatus::Error;
                            state.error = Some(reason.clone());
                            Err(reason)
                        }
                        Ok(Completion::Download(Ok(plan))) => {
                            capability = ReleaseCapability::Downloaded { candidate: capability.candidate(), plan: plan.bind(&self.bus, operation.channel.nightly()) };
                            state.status = UpdateStatus::Ready;
                            Ok(())
                        }
                        Err(error) => {
                            let reason = format!("The update worker failed: {error}");
                            state.status = UpdateStatus::Error;
                            state.error = Some(reason.clone());
                            Err(reason)
                        }
                    };
                    if let Err(reason) = &outcome {
                        if state.status == UpdateStatus::Unavailable {
                            tracing::debug!(event = "updates_unavailable", "{reason}");
                        } else {
                            tracing::warn!(event = "update_operation_failed", "{reason}");
                        }
                    } else {
                        match state.status {
                            UpdateStatus::Available => tracing::info!(
                                event = "update_available",
                                "An application update is available: {}.",
                                state.available_version.as_deref().unwrap_or("unknown version")
                            ),
                            UpdateStatus::Ready if downloaded => tracing::info!(
                                event = "update_downloaded",
                                "The application update has been downloaded and is ready to install."
                            ),
                            UpdateStatus::UpToDate => tracing::info!(
                                event = "update_up_to_date",
                                "The installed application is up to date on the selected update channel."
                            ),
                            _ => tracing::debug!(event = "update_initialized", "Application updates initialized."),
                        }
                    }
                    self.publish(&state);
                    if let Some(reply) = operation.reply { let _ = reply.send(outcome); }
                }
            }
            if automatic_check == AutomaticCheck::Queued
                && !worker.running()
                && automatic
                && retained.borrow().settings.automatically_check_for_updates
                && UpdateChannel::from(retained.borrow().settings.include_nightly_updates) == channel
            {
                automatic_check = AutomaticCheck::None;
                // Keep a verified download while refreshing the same channel's release feed.
                state.error = None;
                state.status = UpdateStatus::Checking;
                self.publish(&state);
                worker = UpdateWorker::Checking(self.start_check(channel, true, None));
            }
        }
    }
    fn start_check(&self, channel: UpdateChannel, network: bool, reply: Option<Reply>) -> Pending {
        let backend = self.backend.clone();
        Pending {
            task: tokio::task::spawn_blocking(move || {
                Completion::Check(backend.check(channel.nightly(), network), network)
            }),
            channel,
            reply,
        }
    }
    fn publish(&self, state: &UpdateState) {
        self.bus.publish(AppEvent::Updates(state.clone()));
    }
}

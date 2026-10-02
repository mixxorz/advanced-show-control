use super::backend::{Backend, BackendError, Candidate, Checked, VelopackBackend};
use super::{UpdateInstallPlan, UpdateState, UpdateStatus, UpdatesCommand, UpdatesHandle};
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
    epoch: u64,
    reply: Option<Reply>,
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
     * Retained channel-policy revision changes MUST clear available and downloaded intent and
     * reject pending replies, including coalesced toggles back to the original channel.
     * Stale blocking completions MUST NOT publish state, log success, or supply installation plans.
     * Settings MUST remain responsive during blocking I/O.
     */
    /**
     * @cc [owner:mixxorz,label:safety;updates] explicit-download-and-install-intent
     * Automatic checks MUST honor the latest retained setting. Check and Download MUST never
     * install, and only explicit Download may download.
     */
    async fn run(mut self) {
        let mut retained = self.bus.state();
        let (settings, initial_revision) = {
            let snapshot = retained.borrow_and_update();
            (snapshot.settings.clone(), snapshot.update_channel_revision)
        };
        let mut nightly = settings.include_nightly_updates;
        let mut automatic = settings.automatically_check_for_updates;
        let mut state = UpdateState::default();
        let mut candidate: Option<Arc<dyn Candidate>> = None;
        let mut staged = None;
        let mut epoch = initial_revision;
        let mut pending = Some(self.start_check(nightly, automatic, epoch, None));
        let mut queued_auto = false;
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
                    let (settings, next_revision) = {
                        let snapshot = retained.borrow_and_update();
                        (snapshot.settings.clone(), snapshot.update_channel_revision)
                    };
                    let channel_changed = epoch != next_revision;
                    let newly_enabled = !automatic && settings.automatically_check_for_updates;
                    automatic = settings.automatically_check_for_updates;
                    if !automatic { queued_auto = false; }
                    if channel_changed {
                        nightly = settings.include_nightly_updates;
                        epoch = next_revision;
                        candidate = None;
                        staged = None;
                        if let Some(operation) = &mut pending
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
                        queued_auto = automatic;
                    } else if newly_enabled { queued_auto = true; }
                }
                _ = timer.tick() => { queued_auto = automatic; }
                command = self.commands.recv() => {
                    let Some(command) = command else { break; };
                    match command {
                        UpdatesCommand::Check { reply } => {
                            if pending.is_some() {
                                let _ = reply.send(Err("An update operation is already in progress.".into()));
                                continue;
                            }
                            staged = None;
                            candidate = None;
                            state.available_version = None;
                            state.error = None;
                            state.status = UpdateStatus::Checking;
                            self.publish(&state);
                            pending = Some(self.start_check(nightly, true, epoch, Some(reply)));
                        }
                        UpdatesCommand::Download { reply } => {
                            if pending.is_some() {
                                let _ = reply.send(Err("An update operation is already in progress.".into()));
                                continue;
                            }
                            let Some(update) = candidate.clone() else {
                                let _ = reply.send(Err("No update is available to download.".into()));
                                continue;
                            };
                            staged = None;
                            state.status = UpdateStatus::Downloading;
                            state.error = None;
                            self.publish(&state);
                            pending = Some(Pending {
                                task: tokio::task::spawn_blocking(move || Completion::Download(update.download())),
                                epoch,
                                reply: Some(reply),
                            });
                        }
                        UpdatesCommand::PrepareInstall { reply } => {
                            // Read retained policy as well as watch notifications before handing out intent.
                            if retained.borrow().update_channel_revision != epoch {
                                let _ = reply.send(Err("The update channel changed; please try again.".into()));
                            } else {
                                let result = staged.as_ref().cloned().ok_or_else(|| "No downloaded update is ready to install.".into());
                                let _ = reply.send(result);
                            }
                        }
                    }
                }
                result = async { (&mut pending.as_mut().expect("pending operation").task).await }, if pending.is_some() => {
                    let operation = pending.take().unwrap();
                    // Retained settings are authoritative even if watch delivery loses the select race.
                    if operation.epoch != epoch || retained.borrow().update_channel_revision != operation.epoch {
                        if let Some(reply) = operation.reply { let _ = reply.send(Err("The update channel changed; please try again.".into())); }
                        if queued_auto && automatic
                            && retained.borrow().settings.automatically_check_for_updates
                            && retained.borrow().update_channel_revision == epoch
                        {
                            queued_auto = false;
                            state.status = UpdateStatus::Checking;
                            self.publish(&state);
                            pending = Some(self.start_check(nightly, true, epoch, None));
                        }
                        continue;
                    }
                    let downloaded = matches!(&result, Ok(Completion::Download(Ok(_))));
                    let outcome = match result {
                        Ok(Completion::Check(Ok(checked), network)) => {
                            state.current_version = Some(checked.current);
                            let same_downloaded_version = staged.is_some()
                                && candidate.as_ref().map(|update| update.version())
                                    == checked.candidate.as_ref().map(|update| update.version());
                            if !same_downloaded_version {
                                staged = None;
                                candidate = checked.candidate;
                            }
                            state.available_version = candidate.as_ref().map(|update| update.version());
                            state.status = if same_downloaded_version {
                                UpdateStatus::Ready
                            } else if candidate.is_some() {
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
                            staged = Some(plan.bind(&self.bus, epoch));
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
            if queued_auto
                && pending.is_none()
                && automatic
                && retained.borrow().settings.automatically_check_for_updates
                && retained.borrow().update_channel_revision == epoch
            {
                queued_auto = false;
                // Keep a verified download while refreshing the same channel's release feed.
                state.error = None;
                state.status = UpdateStatus::Checking;
                self.publish(&state);
                pending = Some(self.start_check(nightly, true, epoch, None));
            }
        }
    }
    fn start_check(
        &self,
        nightly: bool,
        network: bool,
        epoch: u64,
        reply: Option<Reply>,
    ) -> Pending {
        let backend = self.backend.clone();
        Pending {
            task: tokio::task::spawn_blocking(move || {
                Completion::Check(backend.check(nightly, network), network)
            }),
            epoch,
            reply,
        }
    }
    fn publish(&self, state: &UpdateState) {
        self.bus.publish(AppEvent::Updates(state.clone()));
    }
}

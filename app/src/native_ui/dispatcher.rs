use std::future::Future;
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use tokio::sync::mpsc;

use crate::application::{ApplicationCommandContext, complete_cued_cue_recall_typed};
use crate::connection_state::Lv1SystemIdentity;
use crate::cue_lists::CueRecallResult;
use crate::lv1::TcpConnectProbeResult;
use crate::projector::{AppViewState, ProjectionSubscription};
use crate::runtime::errors::AppCommandError;
use crate::show::ShowSessionState;

#[derive(Debug)]
pub enum UiEvent {
    Snapshot(Box<AppViewState>),
    CommandStarted {
        command_id: u64,
    },
    CommandFinished {
        command_id: u64,
        result: Result<(), String>,
    },
    CueRecallFinished {
        command_id: u64,
        session_revision: u64,
        canceled_by_session_replacement: bool,
        result: Result<CueRecallResult, String>,
    },
    SaveDestinationRequired {
        command_id: u64,
    },
    SessionStateQueryFinished {
        query_id: u64,
        persisted_edit_epoch: u64,
        result: Result<ShowSessionState, String>,
    },
    LatencyMeasured {
        session_id: u64,
        identity: Lv1SystemIdentity,
        result: Result<TcpConnectProbeResult, String>,
    },
}

#[derive(Clone)]
pub struct CommandDispatcher {
    runtime: tokio::runtime::Handle,
    commands: ApplicationCommandContext,
    ui_events: mpsc::UnboundedSender<UiEvent>,
    serial_commands: mpsc::UnboundedSender<SerialCommand>,
    next_command_id: Arc<AtomicU64>,
    persisted_edit_epoch: Arc<AtomicU64>,
}

type CommandFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;
type BoxedCommand = Box<dyn FnOnce(ApplicationCommandContext) -> CommandFuture + Send>;

struct SerialCommand {
    command_id: u64,
    command: BoxedCommand,
}

impl CommandDispatcher {
    pub fn new(
        runtime: tokio::runtime::Handle,
        commands: ApplicationCommandContext,
        ui_events: mpsc::UnboundedSender<UiEvent>,
    ) -> Self {
        let (serial_commands, mut serial_receiver) = mpsc::unbounded_channel::<SerialCommand>();
        let serial_context = commands.clone();
        let serial_events = ui_events.clone();
        runtime.spawn(async move {
            while let Some(serial) = serial_receiver.recv().await {
                let result = (serial.command)(serial_context.clone()).await;
                let _ = serial_events.send(UiEvent::CommandFinished {
                    command_id: serial.command_id,
                    result,
                });
            }
        });
        Self {
            runtime,
            commands,
            ui_events,
            serial_commands,
            next_command_id: Arc::new(AtomicU64::new(0)),
            persisted_edit_epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    fn next_command_id(&self) -> u64 {
        self.next_command_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("application command identifier exhausted")
            + 1
    }

    #[cfg(feature = "debug-tools")]
    pub(crate) fn dispatched_count(&self) -> u64 {
        self.next_command_id.load(Ordering::Relaxed)
    }

    pub fn dispatch<F, Fut>(&self, command: F) -> u64
    where
        F: FnOnce(ApplicationCommandContext) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let command_id = self.next_command_id();
        let commands = self.commands.clone();
        let ui_events = self.ui_events.clone();
        let _ = ui_events.send(UiEvent::CommandStarted { command_id });
        self.runtime.spawn(async move {
            let result = command(commands).await;
            let _ = ui_events.send(UiEvent::CommandFinished { command_id, result });
        });
        command_id
    }

    /// @cc [owner:mixxorz,label:safety;ordering] ordered-go-dispatch
    /// A GO command MUST be synchronously admitted to the cue mailbox before this method returns and
    /// before its response waiter is spawned. Command-started and command-finished events MUST retain
    /// the same command ID whether admission succeeds or fails.
    pub fn dispatch_cued_cue_recall(&self, session_revision: u64) -> u64 {
        let command_id = self.next_command_id();
        let _ = self.ui_events.send(UiEvent::CommandStarted { command_id });
        match self.commands.enqueue_cued_cue_recall() {
            Ok(response) => {
                let ui_events = self.ui_events.clone();
                self.runtime.spawn(async move {
                    let result = complete_cued_cue_recall_typed(response).await;
                    let canceled_by_session_replacement = matches!(
                        &result,
                        Err(AppCommandError::RecallCanceled(reason))
                            if reason == "session was replaced"
                    );
                    let result = result.map_err(|error| match error {
                        AppCommandError::CommandFailed(message) => message,
                        other => other.to_string(),
                    });
                    let _ = ui_events.send(UiEvent::CueRecallFinished {
                        command_id,
                        session_revision,
                        canceled_by_session_replacement,
                        result,
                    });
                });
            }
            Err(error) => {
                let _ = self.ui_events.send(UiEvent::CueRecallFinished {
                    command_id,
                    session_revision,
                    canceled_by_session_replacement: false,
                    result: Err(error),
                });
            }
        }
        command_id
    }

    /// @cc [owner:mixxorz,label:persistence;ordering] persisted-ui-mutations-before-file-preflight
    /// Every native UI submission that can persistently mutate Scenes, Cue Lists, or Show lockout
    /// MUST use `dispatch_persisted_edit`, which enters this lane. Destructive-session preflight
    /// queries and save/new/open/template file operations MUST enter this lane without incrementing
    /// the edit epoch. FIFO completion MUST ensure a retried preflight observes the edit that made
    /// its earlier result stale.
    /// GO recall and latency, connection, discovery, and other network operations MUST NOT be routed
    /// through this lane solely for this ordering guarantee.
    pub fn dispatch_serial<F, Fut>(&self, command: F) -> u64
    where
        F: FnOnce(ApplicationCommandContext) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let command_id = self.next_command_id();
        self.enqueue_serial(
            command_id,
            Box::new(move |commands| Box::pin(command(commands))),
        );
        command_id
    }

    /// @cc [owner:mixxorz,label:persistence;ordering] persisted-edit-submission-epoch
    /// This method MUST increment the shared epoch before enqueueing the mutation on the serial
    /// lane. Queries MUST capture that epoch when submitted so the UI can reject a result when any
    /// later persisted edit was submitted, including while the query was in flight.
    pub fn dispatch_persisted_edit<F, Fut>(&self, command: F) -> u64
    where
        F: FnOnce(ApplicationCommandContext) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        self.persisted_edit_epoch.fetch_add(1, Ordering::SeqCst);
        self.dispatch_serial(command)
    }

    pub fn persisted_edit_epoch(&self) -> u64 {
        self.persisted_edit_epoch.load(Ordering::SeqCst)
    }

    pub fn persisted_session_revision(&self) -> u64 {
        self.commands.persisted_session_revision()
    }

    /// @cc [owner:mixxorz,label:product;persistence] guarded-quit-admission
    /// The quit callback MUST run synchronously under exact persisted-revision admission. A
    /// mismatch MUST leave it uncalled so the UI can restart preflight or cancel visibly.
    pub fn admit_guarded_quit(&self, expected_revision: u64, quit: impl FnOnce()) -> bool {
        self.commands
            .admit_persisted_session_revision(expected_revision, quit)
            .is_some()
    }

    pub fn query_show_session_state(&self) -> u64 {
        let query_id = self.next_command_id();
        let persisted_edit_epoch = self.persisted_edit_epoch();
        let ui_events = self.ui_events.clone();
        self.enqueue_serial(
            query_id,
            Box::new(move |commands| {
                Box::pin(async move {
                    let result = commands.current_show_session_state().await;
                    let _ = ui_events.send(UiEvent::SessionStateQueryFinished {
                        query_id,
                        persisted_edit_epoch,
                        result,
                    });
                    Ok(())
                })
            }),
        );
        query_id
    }

    pub fn save_show(&self) -> u64 {
        let command_id = self.next_command_id();
        let ui_events = self.ui_events.clone();
        self.enqueue_serial(
            command_id,
            Box::new(move |commands| {
                Box::pin(async move {
                    if commands.save_show_file(None).await?.is_none() {
                        let _ = ui_events.send(UiEvent::SaveDestinationRequired { command_id });
                    }
                    Ok(())
                })
            }),
        );
        command_id
    }

    fn enqueue_serial(&self, command_id: u64, command: BoxedCommand) {
        let _ = self.ui_events.send(UiEvent::CommandStarted { command_id });
        let serial = SerialCommand {
            command_id,
            command,
        };
        if self.serial_commands.send(serial).is_err() {
            let _ = self.ui_events.send(UiEvent::CommandFinished {
                command_id,
                result: Err("Application command queue unavailable".to_string()),
            });
        }
    }

    pub fn probe_latency(
        &self,
        session_id: u64,
        identity: Lv1SystemIdentity,
        timeout_ms: Option<u64>,
    ) {
        let commands = self.commands.clone();
        let ui_events = self.ui_events.clone();
        self.runtime.spawn(async move {
            let result = commands
                .probe_lv1_tcp_connect_latency(identity.clone(), timeout_ms)
                .await;
            let _ = ui_events.send(UiEvent::LatencyMeasured {
                session_id,
                identity,
                result,
            });
        });
    }

    /// @cc [owner:mixxorz,label:architecture] projection-bridge-latest-snapshot
    /// The bridge MUST forward complete projector snapshots without constructing or mutating them;
    /// presentation applies `state_version` ordering after receiving each value.
    pub fn bridge_projections(&self, mut projections: ProjectionSubscription) {
        let ui_events = self.ui_events.clone();
        self.runtime.spawn(async move {
            loop {
                if projections.changed().await.is_err() {
                    break;
                }
                if let Some(snapshot) = projections.latest()
                    && ui_events
                        .send(UiEvent::Snapshot(Box::new(snapshot)))
                        .is_err()
                {
                    break;
                }
            }
        });
    }
}

pub fn ui_event_channel() -> (
    mpsc::UnboundedSender<UiEvent>,
    mpsc::UnboundedReceiver<UiEvent>,
) {
    mpsc::unbounded_channel()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::lifecycle::AppLifecycle;
    use crate::runtime::events::AppEventBus;
    use crate::settings::SettingsCommand;
    use crate::show::build_show_actor;

    #[tokio::test]
    async fn query_captures_epoch_before_a_later_persisted_edit_submission() {
        let event_bus = AppEventBus::default();
        let (show, show_task, show_peers, lockout) = build_show_actor(event_bus.clone());
        let (settings, _settings_rx) = mpsc::channel::<SettingsCommand>(1);
        let lifecycle = AppLifecycle::new(
            event_bus,
            show.clone(),
            show_peers,
            lockout,
            settings.clone(),
        );
        show_task.spawn();
        let (ui_logs, _) = tokio::sync::broadcast::channel(1);
        let commands = ApplicationCommandContext::new(lifecycle, show, settings, ui_logs);
        let (events, mut event_rx) = ui_event_channel();
        let dispatcher =
            CommandDispatcher::new(tokio::runtime::Handle::current(), commands, events);
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();

        dispatcher.dispatch_serial(|_| async move {
            let _ = release_rx.await;
            Ok(())
        });
        let query_id = dispatcher.query_show_session_state();
        dispatcher.dispatch_persisted_edit(|commands| async move {
            commands
                .create_cue_list("Later edit".to_string())
                .await
                .map(|_| ())
        });
        assert_eq!(dispatcher.persisted_edit_epoch(), 1);
        release_tx.send(()).unwrap();

        let query_epoch = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let UiEvent::SessionStateQueryFinished {
                    query_id: completed_id,
                    persisted_edit_epoch,
                    ..
                } = event_rx.recv().await.expect("UI event channel closed")
                    && completed_id == query_id
                {
                    break persisted_edit_epoch;
                }
            }
        })
        .await
        .expect("session-state query timed out");

        assert_eq!(query_epoch, 0);
    }

    #[tokio::test]
    async fn guarded_quit_calls_closure_only_for_exact_revision() {
        let event_bus = AppEventBus::default();
        let event_bus_for_increment = event_bus.clone();
        let (show, show_task, show_peers, lockout) = build_show_actor(event_bus.clone());
        let (settings, _settings_rx) = mpsc::channel::<SettingsCommand>(1);
        let lifecycle = AppLifecycle::new(
            event_bus,
            show.clone(),
            show_peers,
            lockout,
            settings.clone(),
        );
        show_task.spawn();
        let (ui_logs, _) = tokio::sync::broadcast::channel(1);
        let commands = ApplicationCommandContext::new(lifecycle, show, settings, ui_logs);
        let (events, _event_rx) = ui_event_channel();
        let dispatcher =
            CommandDispatcher::new(tokio::runtime::Handle::current(), commands, events);
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));

        let admitted_calls = calls.clone();
        assert!(dispatcher.admit_guarded_quit(0, move || {
            admitted_calls.fetch_add(1, Ordering::SeqCst);
        }));
        event_bus_for_increment.note_persisted_session_edit();
        let rejected_calls = calls.clone();
        assert!(!dispatcher.admit_guarded_quit(0, move || {
            rejected_calls.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn owner_edit_published_after_query_creation_is_detectable_before_continuation() {
        let event_bus = AppEventBus::default();
        let event_bus_for_publish = event_bus.clone();
        let (show, show_task, show_peers, lockout) = build_show_actor(event_bus.clone());
        let (settings, _settings_rx) = mpsc::channel::<SettingsCommand>(1);
        let lifecycle = AppLifecycle::new(
            event_bus,
            show.clone(),
            show_peers,
            lockout,
            settings.clone(),
        );
        show_task.spawn();
        let (ui_logs, _) = tokio::sync::broadcast::channel(1);
        let commands = ApplicationCommandContext::new(lifecycle, show, settings, ui_logs);
        let (events, mut event_rx) = ui_event_channel();
        let dispatcher =
            CommandDispatcher::new(tokio::runtime::Handle::current(), commands, events);

        let query_id = dispatcher.query_show_session_state();
        let queried_revision = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let UiEvent::SessionStateQueryFinished {
                    query_id: completed_id,
                    result,
                    ..
                } = event_rx.recv().await.expect("UI event channel closed")
                    && completed_id == query_id
                {
                    break result
                        .expect("session-state query failed")
                        .persisted_session_revision;
                }
            }
        })
        .await
        .expect("session-state query timed out");

        event_bus_for_publish.publish(crate::runtime::events::AppEvent::CueLists(
            crate::cue_lists::CueListsProjectionState::default(),
        ));

        assert_ne!(queried_revision, dispatcher.persisted_session_revision());
    }

    #[tokio::test]
    async fn persisted_edit_queued_before_preflight_completes_before_the_query() {
        let event_bus = AppEventBus::default();
        let (show, show_task, show_peers, lockout) = build_show_actor(event_bus.clone());
        let (settings, _settings_rx) = mpsc::channel::<SettingsCommand>(1);
        let lifecycle = AppLifecycle::new(
            event_bus,
            show.clone(),
            show_peers,
            lockout,
            settings.clone(),
        );
        show_task.spawn();
        let (ui_logs, _) = tokio::sync::broadcast::channel(1);
        let commands = ApplicationCommandContext::new(lifecycle, show, settings, ui_logs);
        let (events, mut event_rx) = ui_event_channel();
        let dispatcher =
            CommandDispatcher::new(tokio::runtime::Handle::current(), commands, events);

        dispatcher.dispatch_persisted_edit(|commands| async move {
            commands
                .create_cue_list("Ordered edit".to_string())
                .await
                .map(|_| ())
        });
        let query_id = dispatcher.query_show_session_state();

        let result = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let UiEvent::SessionStateQueryFinished {
                    query_id: completed_id,
                    result,
                    ..
                } = event_rx.recv().await.expect("UI event channel closed")
                    && completed_id == query_id
                {
                    break result;
                }
            }
        })
        .await
        .expect("session-state query timed out")
        .expect("session-state query failed");

        assert!(result.show_file_dirty);
    }
}

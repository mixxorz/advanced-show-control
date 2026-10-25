use std::future::Future;
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use tokio::sync::mpsc;

use crate::application::{ApplicationCommandContext, complete_cued_cue_recall_typed};
use crate::connection_state::Lv1SystemIdentity;
use crate::cue_lists::CueRecallResult;
use crate::lv1::TcpConnectProbeResult;
use crate::projector::{AppViewState, ProjectionSubscription};
use crate::runtime::errors::AppCommandError;
use crate::show::ShowSessionState;
use crate::updates::UpdateInstallPlan;

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
        result: Result<CueRecallResult, String>,
    },
    SaveDestinationRequired {
        command_id: u64,
    },
    SessionStateQueryFinished {
        query_id: u64,
        result: Result<ShowSessionState, String>,
    },
    UpdateInstallPrepared {
        command_id: u64,
        result: Result<UpdateInstallPlan, String>,
    },
    UpdateShutdownPrepared {
        command_id: u64,
        result: Result<UpdateInstallPlan, String>,
    },
    LatencyMeasured {
        session_id: u64,
        attempt_id: u64,
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
    session_transition_pending: Arc<AtomicBool>,
    #[cfg(any(test, feature = "debug-tools"))]
    latency_probe_override: Option<LatencyProbeOverride>,
}

#[cfg(any(test, feature = "debug-tools"))]
type LatencyProbeOverride =
    Arc<dyn Fn(u64, u64, Lv1SystemIdentity, Option<u64>) + Send + Sync + 'static>;

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
            session_transition_pending: Arc::new(AtomicBool::new(false)),
            #[cfg(any(test, feature = "debug-tools"))]
            latency_probe_override: None,
        }
    }

    fn next_command_id(&self) -> u64 {
        self.next_command_id
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
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
    /// A GO command MUST be rejected while a UI session transition is pending. Otherwise it MUST be
    /// synchronously admitted to the cue mailbox before this method returns and before its response
    /// waiter is spawned. Command-started and command-finished events MUST retain
    /// the same command ID whether admission succeeds or fails.
    pub fn dispatch_cued_cue_recall(&self) -> u64 {
        let command_id = self.next_command_id();
        let _ = self.ui_events.send(UiEvent::CommandStarted { command_id });
        let admission = if self.session_transition_pending() {
            Err("The session action is still in progress.".to_string())
        } else {
            self.commands.enqueue_cued_cue_recall()
        };
        match admission {
            Ok(response) => {
                let ui_events = self.ui_events.clone();
                self.runtime.spawn(async move {
                    let result = complete_cued_cue_recall_typed(response).await;
                    let result = result.map_err(|error| match error {
                        AppCommandError::CommandFailed(message) => message,
                        other => other.to_string(),
                    });
                    let _ = ui_events.send(UiEvent::CueRecallFinished { command_id, result });
                });
            }
            Err(error) => {
                let _ = self.ui_events.send(UiEvent::CueRecallFinished {
                    command_id,
                    result: Err(error),
                });
            }
        }
        command_id
    }

    /// @cc [owner:mixxorz,label:persistence;ordering] persisted-ui-mutations-before-file-preflight
    /// Every native UI submission that can persistently mutate Scenes, Cue Lists, or Show lockout
    /// MUST use `dispatch_persisted_edit`, which enters this lane. Destructive-session preflight
    /// queries and save/new/open/template file operations MUST enter this lane. FIFO completion
    /// MUST ensure preflight observes UI edits submitted before the session action began.
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

    /// @cc [owner:mixxorz,label:product;persistence] pending-session-blocks-ui-edits
    /// New UI document edit submissions MUST be rejected while SessionGuard is pending, including
    /// file pickers and continued asynchronous file operations. This is UI busy behavior only; it
    /// does not cancel or revoke work accepted before the session action began.
    pub fn dispatch_persisted_edit<F, Fut>(&self, command: F) -> u64
    where
        F: FnOnce(ApplicationCommandContext) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        if self.session_transition_pending() {
            let command_id = self.next_command_id();
            let _ = self.ui_events.send(UiEvent::CommandStarted { command_id });
            let _ = self.ui_events.send(UiEvent::CommandFinished {
                command_id,
                result: Err("The session action is still in progress.".to_string()),
            });
            return command_id;
        }
        self.dispatch_serial(command)
    }

    pub(super) fn session_transition_indicator(&self) -> Arc<AtomicBool> {
        self.session_transition_pending.clone()
    }

    pub(super) fn session_transition_pending(&self) -> bool {
        self.session_transition_pending.load(Ordering::Relaxed)
    }

    pub fn prepare_update_install(&self) -> u64 {
        let command_id = self.next_command_id();
        let commands = self.commands.clone();
        let ui_events = self.ui_events.clone();
        let _ = ui_events.send(UiEvent::CommandStarted { command_id });
        self.runtime.spawn(async move {
            let result = commands.prepare_update_install().await;
            let _ = ui_events.send(UiEvent::UpdateInstallPrepared { command_id, result });
        });
        command_id
    }

    pub fn disconnect_for_update(&self, plan: UpdateInstallPlan) -> u64 {
        let command_id = self.next_command_id();
        let commands = self.commands.clone();
        let ui_events = self.ui_events.clone();
        let _ = ui_events.send(UiEvent::CommandStarted { command_id });
        self.runtime.spawn(async move {
            let result = commands.disconnect_lv1().await.map(|_| plan);
            let _ = ui_events.send(UiEvent::UpdateShutdownPrepared { command_id, result });
        });
        command_id
    }

    pub fn arm_update_install(&self, plan: UpdateInstallPlan) -> Result<(), String> {
        self.commands.arm_update_install(plan)
    }

    pub fn query_show_session_state(&self) -> u64 {
        let query_id = self.next_command_id();
        let ui_events = self.ui_events.clone();
        self.enqueue_serial(
            query_id,
            Box::new(move |commands| {
                Box::pin(async move {
                    let result = commands.current_show_session_state().await;
                    let _ = ui_events.send(UiEvent::SessionStateQueryFinished { query_id, result });
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

    #[cfg(any(test, feature = "debug-tools"))]
    pub(crate) fn with_latency_probe_override(
        mut self,
        probe: impl Fn(u64, u64, Lv1SystemIdentity, Option<u64>) + Send + Sync + 'static,
    ) -> Self {
        self.latency_probe_override = Some(Arc::new(probe));
        self
    }

    pub fn probe_latency(
        &self,
        session_id: u64,
        attempt_id: u64,
        identity: Lv1SystemIdentity,
        timeout_ms: Option<u64>,
    ) {
        #[cfg(any(test, feature = "debug-tools"))]
        if let Some(probe) = &self.latency_probe_override {
            probe(session_id, attempt_id, identity, timeout_ms);
            return;
        }

        let commands = self.commands.clone();
        let ui_events = self.ui_events.clone();
        self.runtime.spawn(async move {
            let result = commands
                .probe_lv1_tcp_connect_latency(identity.clone(), timeout_ms)
                .await;
            let _ = ui_events.send(UiEvent::LatencyMeasured {
                session_id,
                attempt_id,
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
    async fn preflight_orders_prior_edits_and_blocks_new_edits_and_go() {
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
        let mut guard = super::super::session_guard::SessionGuard::new(
            dispatcher.session_transition_indicator(),
        );
        guard.request(super::super::session_guard::SessionAction::Open);
        let rejected_edit = dispatcher.dispatch_persisted_edit(|_| async {
            panic!("A pending session action must reject document edits before invoking them");
        });
        let rejected_go = dispatcher.dispatch_cued_cue_recall();
        let query_id = dispatcher.query_show_session_state();
        let mut edit_blocked = false;
        let mut go_blocked = false;

        let result = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                match event_rx.recv().await.expect("UI event channel closed") {
                    UiEvent::CommandFinished { command_id, result }
                        if command_id == rejected_edit =>
                    {
                        assert_eq!(
                            result,
                            Err("The session action is still in progress.".to_string())
                        );
                        edit_blocked = true;
                    }
                    UiEvent::CueRecallFinished { command_id, result }
                        if command_id == rejected_go =>
                    {
                        assert_eq!(
                            result.unwrap_err(),
                            "The session action is still in progress."
                        );
                        go_blocked = true;
                    }
                    UiEvent::SessionStateQueryFinished {
                        query_id: completed_id,
                        result,
                    } if completed_id == query_id => break result,
                    _ => {}
                }
            }
        })
        .await
        .expect("session-state query timed out")
        .expect("session-state query failed");

        assert!(result.show_file_dirty);
        assert!(edit_blocked);
        assert!(go_blocked);
        assert!(dispatcher.session_transition_pending());
        guard.cancel_pending();
        assert!(!dispatcher.session_transition_pending());
    }
}

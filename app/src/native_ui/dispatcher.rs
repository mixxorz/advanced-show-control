use std::future::Future;
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use tokio::sync::mpsc;

use crate::application::ApplicationCommandContext;
use crate::connection_state::Lv1SystemIdentity;
use crate::lv1::TcpConnectProbeResult;
use crate::projector::{AppViewState, ProjectionSubscription};

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
    SaveDestinationRequired {
        command_id: u64,
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
        }
    }

    fn next_command_id(&self) -> u64 {
        self.next_command_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("application command identifier exhausted")
            + 1
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

    /// Enqueues commands that must reach their owner in user-action order. Settings replacements use
    /// this lane so complete-object edits compose, and file actions use it so a subsequent Save sees
    /// the authoritative result of the preceding New, Open, or template load.
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

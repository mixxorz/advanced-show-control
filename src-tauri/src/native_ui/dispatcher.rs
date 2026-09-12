use std::future::Future;
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
    LatencyMeasured {
        identity: Lv1SystemIdentity,
        result: Result<TcpConnectProbeResult, String>,
    },
}

#[derive(Clone)]
pub struct CommandDispatcher {
    runtime: tokio::runtime::Handle,
    commands: ApplicationCommandContext,
    ui_events: mpsc::UnboundedSender<UiEvent>,
    next_command_id: Arc<AtomicU64>,
}

impl CommandDispatcher {
    pub fn new(
        runtime: tokio::runtime::Handle,
        commands: ApplicationCommandContext,
        ui_events: mpsc::UnboundedSender<UiEvent>,
    ) -> Self {
        Self {
            runtime,
            commands,
            ui_events,
            next_command_id: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn dispatch<F, Fut>(&self, command: F) -> u64
    where
        F: FnOnce(ApplicationCommandContext) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let command_id = self
            .next_command_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("application command identifier exhausted")
            + 1;
        let commands = self.commands.clone();
        let ui_events = self.ui_events.clone();
        let _ = ui_events.send(UiEvent::CommandStarted { command_id });
        self.runtime.spawn(async move {
            let result = command(commands).await;
            let _ = ui_events.send(UiEvent::CommandFinished { command_id, result });
        });
        command_id
    }

    pub fn probe_latency(&self, identity: Lv1SystemIdentity, timeout_ms: Option<u64>) {
        let commands = self.commands.clone();
        let ui_events = self.ui_events.clone();
        self.runtime.spawn(async move {
            let result = commands
                .probe_lv1_tcp_connect_latency(identity.clone(), timeout_ms)
                .await;
            let _ = ui_events.send(UiEvent::LatencyMeasured { identity, result });
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

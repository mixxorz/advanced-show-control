//! App runtime lifecycle ownership.

use std::sync::Arc;

use tauri::{AppHandle, Runtime};
use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;

use crate::fade::{FadeEngineHandle, spawn_engine};
use crate::logging::UiLogEvent;
use crate::lv1::{ConnectionStatus, Lv1ActorHandle, Lv1Command, Lv1Event, spawn_actor};
use crate::runtime::commands::AppCommandBus;
use crate::runtime::events::{AppEvent, AppEventBus, RuntimeLifecycleEvent};
use crate::scene_recall::spawn_scene_recall_fader;
use crate::show::{
    ConnectCommandResult, ShowCommandResult, ShowStateHandle, spawn_lv1_scene_list_monitor,
};

#[derive(Default)]
pub struct RuntimeHandles {
    pub lv1: Option<Lv1ActorHandle>,
    pub fade: Option<FadeEngineHandle>,
    pub scene_recall_fader: Option<JoinHandle<()>>,
    pub show_scene_list_monitor: Option<JoinHandle<()>>,
}

impl RuntimeHandles {
    pub fn with_runtime_targets(lv1: Lv1ActorHandle, fade: FadeEngineHandle) -> Self {
        Self {
            lv1: Some(lv1),
            fade: Some(fade),
            ..Default::default()
        }
    }

    pub fn abort_all(&mut self) {
        if let Some(handle) = self.scene_recall_fader.take() {
            handle.abort();
        }
        if let Some(handle) = self.show_scene_list_monitor.take() {
            handle.abort();
        }
        self.lv1 = None;
        self.fade = None;
    }
}

pub enum RuntimeInstallRejection {
    StaleGeneration { handles: RuntimeHandles },
    MissingRuntimeTargets { handles: RuntimeHandles },
}

#[derive(Clone, Copy)]
pub enum ConnectFailureMode {
    ClearConnectedIdentity,
    PreserveConnectedIdentity,
}

impl RuntimeInstallRejection {
    pub fn into_handles(self) -> RuntimeHandles {
        match self {
            Self::StaleGeneration { handles } | Self::MissingRuntimeTargets { handles } => handles,
        }
    }
}

struct LifecycleInner {
    generation: u64,
    connecting: bool,
    frontend_ready: bool,
    handles: RuntimeHandles,
    projector: Option<JoinHandle<()>>,
    show_runtime_metadata_monitor: Option<JoinHandle<()>>,
    command_bus: AppCommandBus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ShowRuntimeMetadataMonitorNotification {
    RuntimeGenerationChanged {
        generation: u64,
    },
    Lv1Disconnected {
        generation: u64,
        active: bool,
    },
    StaleLv1EventIgnored {
        generation: u64,
        active_generation: u64,
    },
}

#[derive(Clone)]
pub struct AppLifecycle {
    inner: Arc<Mutex<LifecycleInner>>,
    event_bus: AppEventBus,
    show: ShowStateHandle,
}

impl AppLifecycle {
    pub fn new(event_bus: AppEventBus, show: ShowStateHandle) -> Self {
        let command_bus = AppCommandBus::new_with_show(show.clone());

        Self {
            inner: Arc::new(Mutex::new(LifecycleInner {
                generation: 0,
                connecting: false,
                frontend_ready: false,
                handles: RuntimeHandles::default(),
                projector: None,
                show_runtime_metadata_monitor: None,
                command_bus,
            })),
            event_bus,
            show,
        }
    }

    pub async fn begin_connecting(&self) -> Option<u64> {
        let mut inner = self.inner.lock().await;
        inner.generation = inner.generation.saturating_add(1);
        inner.connecting = true;
        let generation = inner.generation;
        drop(inner);
        self.event_bus
            .publish_runtime_generation_changed(generation);
        Some(generation)
    }

    pub async fn active_generation(&self) -> u64 {
        self.inner.lock().await.generation
    }

    pub async fn install_runtime_transaction(
        &self,
        generation: u64,
        handles: RuntimeHandles,
    ) -> Result<(), RuntimeInstallRejection> {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return Err(RuntimeInstallRejection::StaleGeneration { handles });
        }

        let fade = match handles.fade.clone() {
            Some(fade) if handles.lv1.is_some() => fade,
            _ => return Err(RuntimeInstallRejection::MissingRuntimeTargets { handles }),
        };

        inner
            .command_bus
            .set_runtime_targets(generation, fade)
            .await;
        let mut handles = handles;
        handles.show_scene_list_monitor = Some(spawn_lv1_scene_list_monitor(
            self.show.clone(),
            self.event_bus.subscribe(),
        ));
        inner.handles = handles;
        inner.connecting = false;
        Ok(())
    }

    pub async fn clear_runtime_transaction(&self, generation: u64) {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return;
        }
        inner.handles.abort_all();
        inner.command_bus.clear_runtime_targets(generation).await;
        inner.generation = inner.generation.saturating_add(1);
        let generation = inner.generation;
        drop(inner);
        self.event_bus
            .publish_runtime_generation_changed(generation);
    }

    async fn install_scene_recall_fader(&self, generation: u64, handle: JoinHandle<()>) {
        let mut inner = self.inner.lock().await;
        if inner.generation == generation {
            if let Some(previous) = inner.handles.scene_recall_fader.replace(handle) {
                previous.abort();
            }
        } else {
            handle.abort();
        }
    }

    pub async fn abort_current_runtime(&self) {
        let generation = self.active_generation().await;
        self.clear_runtime_transaction(generation).await;
    }

    pub async fn abort_runtime_handles_without_advancing_generation(&self) {
        let (generation, command_bus) = {
            let mut inner = self.inner.lock().await;
            inner.handles.abort_all();
            (inner.generation, inner.command_bus.clone())
        };
        command_bus.clear_runtime_targets(generation).await;
    }

    pub async fn connect_to_identity<R: Runtime>(
        &self,
        app: AppHandle<R>,
        generation: u64,
        identity: crate::connection_state::Lv1SystemIdentity,
        failure_mode: ConnectFailureMode,
    ) -> Result<ConnectCommandResult, String> {
        log_lv1_connect_requested(&identity);
        log_lv1_connecting(&identity);
        let event_bus = self.event_bus.clone();
        let command_bus = self.current_command_bus().await;
        let lv1 = spawn_actor(
            identity.address.clone(),
            identity.port,
            event_bus.clone(),
            generation,
        );
        let fade = spawn_engine(
            command_bus.clone(),
            lv1.clone(),
            event_bus.clone(),
            generation,
        );
        let handles = RuntimeHandles::with_runtime_targets(lv1.clone(), fade.clone());
        if let Err(rejection) = self.install_runtime_transaction(generation, handles).await {
            let mut handles = rejection.into_handles();
            handles.abort_all();
            return Err("generation is stale".to_string());
        }

        let (reply, rx) = oneshot::channel();
        lv1.send(Lv1Command::GetState { reply })
            .await
            .map_err(|error| error.to_string())?;
        let initial_snapshot = rx.await.map_err(|_| {
            crate::runtime::commands::AppCommandError::ReplyChannelClosed.to_string()
        })?;

        if initial_snapshot.connection != ConnectionStatus::Connected {
            self.clear_runtime_transaction(generation).await;
            let _ = self
                .apply_failed_connect_metadata(&command_bus, failure_mode)
                .await;
            log_lv1_connect_failed(&identity, failure_mode);
            return Err("LV1 did not connect".to_string());
        }

        let reconnect_state = crate::connection_state::ReconnectState::default();
        let connect_result = self
            .apply_connected_lv1_metadata(&command_bus, identity.clone(), reconnect_state)
            .await
            .map_err(|error| error.to_string())?;
        log_lv1_connected(&identity);
        let _ = lv1;
        let _ = fade;
        let scene_recall_fader = spawn_scene_recall_fader(generation, command_bus, lv1, event_bus);
        self.install_scene_recall_fader(generation, scene_recall_fader)
            .await;
        let _ = app;
        Ok(connect_result)
    }

    #[allow(clippy::too_many_arguments, dead_code)]
    async fn finish_connect_transaction_inner<R: Runtime>(
        &self,
        app: AppHandle<R>,
        identity: crate::connection_state::Lv1SystemIdentity,
        failure_mode: ConnectFailureMode,
        generation: u64,
        command_bus: AppCommandBus,
        event_bus: AppEventBus,
        lv1: crate::lv1::Lv1ActorHandle,
        fade: crate::fade::FadeEngineHandle,
        before_scene_recall_start: Option<Box<dyn FnOnce(AppCommandBus) + Send>>,
    ) -> Result<ConnectCommandResult, String> {
        let handles = RuntimeHandles::with_runtime_targets(lv1.clone(), fade.clone());

        if self
            .install_runtime_transaction(generation, handles)
            .await
            .is_err()
        {
            return Err("generation is stale".to_string());
        }

        let (reply, rx) = oneshot::channel();
        lv1.send(Lv1Command::GetState { reply })
            .await
            .map_err(|error| error.to_string())?;
        let initial_snapshot = rx.await.map_err(|_| {
            crate::runtime::commands::AppCommandError::ReplyChannelClosed.to_string()
        })?;
        if initial_snapshot.connection != ConnectionStatus::Connected {
            self.clear_runtime_transaction(generation).await;
            let _ = self
                .apply_failed_connect_metadata(&command_bus, failure_mode)
                .await;
            log_lv1_connect_failed(&identity, failure_mode);
            return Err("LV1 did not connect".to_string());
        }

        let reconnect_state = crate::connection_state::ReconnectState::default();
        let connect_result = self
            .apply_connected_lv1_metadata(&command_bus, identity.clone(), reconnect_state)
            .await
            .map_err(|error| error.to_string())?;
        log_lv1_connected(&identity);
        if let Some(before_scene_recall_start) = before_scene_recall_start {
            before_scene_recall_start(command_bus.clone());
        }
        let scene_recall_fader = spawn_scene_recall_fader(generation, command_bus, lv1, event_bus);
        self.install_scene_recall_fader(generation, scene_recall_fader)
            .await;
        let _ = app;
        Ok(connect_result)
    }

    async fn apply_connected_lv1_metadata(
        &self,
        command_bus: &AppCommandBus,
        identity: crate::connection_state::Lv1SystemIdentity,
        reconnect: crate::connection_state::ReconnectState,
    ) -> Result<ConnectCommandResult, crate::runtime::commands::AppCommandError> {
        let pending_result = command_bus.set_pending_lv1_identity(None).await?;
        let connected_result = command_bus
            .establish_connected_lv1_identity(identity)
            .await?;
        let reconnect_result = command_bus.set_reconnect_state(reconnect).await?;
        Ok(ConnectCommandResult {
            changed: pending_result.changed || connected_result.changed || reconnect_result.changed,
        })
    }

    async fn apply_failed_connect_metadata(
        &self,
        command_bus: &AppCommandBus,
        failure_mode: ConnectFailureMode,
    ) -> Result<(), crate::runtime::commands::AppCommandError> {
        match failure_mode {
            ConnectFailureMode::ClearConnectedIdentity => {
                let _ = command_bus.clear_connected_lv1_identity().await?;
            }
            ConnectFailureMode::PreserveConnectedIdentity => {
                let _ = command_bus.set_pending_lv1_identity(None).await?;
                let _ = command_bus
                    .set_reconnect_state(crate::connection_state::ReconnectState::default())
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn disconnect_current_runtime(&self) -> Result<ShowCommandResult, String> {
        tracing::debug!(
            event = "lv1_disconnect_requested",
            "LV1 disconnect requested"
        );
        let generation = self.active_generation().await;
        let reason = "Disconnected by user".to_string();
        self.abort_runtime_handles_without_advancing_generation()
            .await;
        self.event_bus.publish(AppEvent::Lv1 {
            generation,
            event: Lv1Event::Disconnected { reason },
        });
        self.clear_runtime_transaction(generation).await;
        tracing::info!(event = "lv1_disconnected", "Disconnected from LV1");
        Ok(ShowCommandResult { changed: true })
    }

    pub async fn current_command_bus(&self) -> AppCommandBus {
        let (command_bus, show) = {
            let inner = self.inner.lock().await;
            (inner.command_bus.clone(), self.show.clone())
        };
        command_bus.set_show_target(show).await;
        command_bus
    }

    pub async fn current_lv1(&self) -> Option<Lv1ActorHandle> {
        self.inner.lock().await.handles.lv1.clone()
    }

    pub(crate) async fn connected_lv1_identity(
        &self,
    ) -> Option<crate::connection_state::Lv1SystemIdentity> {
        self.show
            .initial_projection_state()
            .await
            .connected_lv1_identity
    }

    pub async fn connect_lv1_system<R: Runtime>(
        &self,
        app: AppHandle<R>,
        identity: crate::connection_state::Lv1SystemIdentity,
    ) -> Result<ConnectCommandResult, String> {
        self.abort_current_runtime().await;
        let generation = self
            .begin_connecting()
            .await
            .ok_or_else(|| "Failed to begin LV1 connection".to_string())?;
        self.connect_to_identity(
            app,
            generation,
            identity,
            ConnectFailureMode::ClearConnectedIdentity,
        )
        .await
    }

    pub async fn attempt_reconnect_lv1<R: Runtime>(
        &self,
        app: AppHandle<R>,
    ) -> Result<ConnectCommandResult, String> {
        let identity = self
            .connected_lv1_identity()
            .await
            .ok_or_else(|| "Reconnect unavailable: no previous LV1 identity".to_string())?;
        self.abort_current_runtime().await;
        let generation = self
            .begin_connecting()
            .await
            .ok_or_else(|| "Failed to begin LV1 reconnect".to_string())?;
        self.connect_to_identity(
            app,
            generation,
            identity,
            ConnectFailureMode::PreserveConnectedIdentity,
        )
        .await
    }

    pub async fn startup_auto_connect_lv1<R: Runtime>(
        &self,
        app: AppHandle<R>,
    ) -> Result<ConnectCommandResult, String> {
        let Some(identity) = self.connected_lv1_identity().await else {
            return Ok(ConnectCommandResult { changed: false });
        };
        self.abort_current_runtime().await;
        let generation = self
            .begin_connecting()
            .await
            .ok_or_else(|| "Failed to begin LV1 startup auto-connect".to_string())?;
        self.connect_to_identity(
            app,
            generation,
            identity,
            ConnectFailureMode::ClearConnectedIdentity,
        )
        .await
    }

    pub async fn frontend_ready<R: Runtime>(
        &self,
        app: AppHandle<R>,
        logs: tokio::sync::broadcast::Receiver<UiLogEvent>,
    ) -> Result<(), String> {
        let initial_show_state = self.show.initial_projection_state().await;
        let mut inner = self.inner.lock().await;
        if inner.frontend_ready {
            return Ok(());
        }
        inner.frontend_ready = true;
        let generation = inner.generation;
        if inner.show_runtime_metadata_monitor.is_none() {
            inner.show_runtime_metadata_monitor = Some(spawn_show_runtime_metadata_monitor(
                self.event_bus.subscribe(),
                inner.command_bus.clone(),
            ));
        }
        inner.projector = Some(crate::projector::spawn_projector(
            crate::projector::ProjectorInputs {
                app,
                generation,
                initial_show_state,
                events: self.event_bus.subscribe(),
                logs,
            },
        ));
        Ok(())
    }

    pub async fn set_command_bus(&self, command_bus: Option<AppCommandBus>) {
        if let Some(command_bus) = command_bus {
            self.inner.lock().await.command_bus = command_bus;
        }
    }
}

impl Default for AppLifecycle {
    fn default() -> Self {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        Self::new(event_bus, show)
    }
}

fn log_lv1_connected(identity: &crate::connection_state::Lv1SystemIdentity) {
    let host = identity
        .host
        .as_deref()
        .unwrap_or(identity.address.as_str());
    tracing::info!(
        event = "lv1_connected",
        host = %host,
        port = identity.port,
        "LV1 connected"
    );
}

fn log_lv1_connect_requested(identity: &crate::connection_state::Lv1SystemIdentity) {
    tracing::debug!(
        event = "lv1_connect_requested",
        host = %identity.address,
        port = identity.port,
        "LV1 connect requested"
    );
}

fn log_lv1_connecting(identity: &crate::connection_state::Lv1SystemIdentity) {
    tracing::info!(
        event = "lv1_connecting",
        host = %identity.address,
        port = identity.port,
        "Connecting to LV1"
    );
}

fn log_lv1_connect_failed(
    identity: &crate::connection_state::Lv1SystemIdentity,
    failure_mode: ConnectFailureMode,
) {
    match failure_mode {
        ConnectFailureMode::ClearConnectedIdentity => tracing::warn!(
            event = "lv1_connect_failed",
            host = %identity.address,
            port = identity.port,
            error = "LV1 did not connect",
            "LV1 did not connect"
        ),
        ConnectFailureMode::PreserveConnectedIdentity => tracing::warn!(
            event = "lv1_reconnect_failed",
            host = %identity.address,
            port = identity.port,
            error = "LV1 did not connect",
            "LV1 did not connect"
        ),
    }
}

fn spawn_show_runtime_metadata_monitor(
    events: tokio::sync::broadcast::Receiver<AppEvent>,
    command_bus: AppCommandBus,
) -> JoinHandle<()> {
    spawn_show_runtime_metadata_monitor_with_notifier(events, command_bus, |_| {})
}

fn spawn_show_runtime_metadata_monitor_with_notifier(
    events: tokio::sync::broadcast::Receiver<AppEvent>,
    command_bus: AppCommandBus,
    notify: impl Fn(ShowRuntimeMetadataMonitorNotification) + Send + 'static,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut events = events;
        let mut active_generation = 0;
        loop {
            match events.recv().await {
                Ok(AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged {
                    generation,
                })) => {
                    active_generation = generation;
                    notify(
                        ShowRuntimeMetadataMonitorNotification::RuntimeGenerationChanged {
                            generation,
                        },
                    );
                }
                Ok(AppEvent::Lv1 { generation, event }) if generation == active_generation => {
                    if let Lv1Event::Disconnected { reason } = event {
                        notify(ShowRuntimeMetadataMonitorNotification::Lv1Disconnected {
                            generation,
                            active: true,
                        });
                        let _ = command_bus.handle_runtime_disconnected(reason).await;
                    }
                }
                Ok(AppEvent::Lv1 { generation, .. }) => {
                    notify(
                        ShowRuntimeMetadataMonitorNotification::StaleLv1EventIgnored {
                            generation,
                            active_generation,
                        },
                    );
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::debug!(skipped, "show runtime metadata monitor lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection_state::ReconnectState;
    use crate::connection_state::{DiscoveredLv1Status, DiscoveredLv1System, Lv1SystemIdentity};
    use crate::fade::FadeEngineHandle;
    use crate::lv1::{Lv1Command, Lv1StateSnapshot, SceneListEntry, test_actor_handle};
    use crate::show::{ShowEvent, ShowProjectionReason};
    use std::sync::{Arc, Mutex as StdMutex};
    use tauri::test::mock_app;
    use tokio::sync::{mpsc, oneshot};
    use tracing::field::{Field, Visit};
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::Context;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::registry::{LookupSpan, Registry};

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    struct CapturedLogEvent {
        event: Option<String>,
        host: Option<String>,
        port: Option<u16>,
    }

    #[derive(Clone, Default)]
    struct CapturedLogEvents(Arc<StdMutex<Vec<CapturedLogEvent>>>);

    impl<S> Layer<S> for CapturedLogEvents
    where
        S: tracing::Subscriber,
        S: for<'a> LookupSpan<'a>,
    {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut visitor = CapturedLogEvent::default();
            event.record(&mut visitor);
            self.0.lock().unwrap().push(visitor);
        }
    }

    impl Visit for CapturedLogEvent {
        fn record_str(&mut self, field: &Field, value: &str) {
            match field.name() {
                "event" => self.event = Some(value.to_string()),
                "host" => self.host = Some(value.to_string()),
                _ => {}
            }
        }

        fn record_u64(&mut self, field: &Field, value: u64) {
            if field.name() == "port" {
                self.port = u16::try_from(value).ok();
            }
        }

        fn record_i64(&mut self, field: &Field, value: i64) {
            if field.name() == "port" {
                self.port = u16::try_from(value).ok();
            }
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            let value = format!("{value:?}").trim_matches('"').to_string();
            match field.name() {
                "event" => self.event = Some(value),
                "host" => self.host = Some(value),
                _ => {}
            }
        }
    }

    fn fake_lv1_handle(snapshot: Lv1StateSnapshot) -> crate::lv1::Lv1ActorHandle {
        let (tx, mut rx) = mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if let Lv1Command::GetState { reply } = command {
                    let _ = reply.send(snapshot.clone());
                }
            }
        });
        test_actor_handle(tx)
    }

    fn connected_snapshot() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![],
            channels: vec![],
        }
    }

    fn disconnected_snapshot() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Disconnected,
            scene: None,
            scene_list: vec![],
            channels: vec![],
        }
    }

    #[tokio::test]
    async fn lifecycle_allocates_monotonic_generations() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus, show);

        let first = lifecycle.begin_connecting().await.unwrap();
        lifecycle.abort_current_runtime().await;
        let second = lifecycle.begin_connecting().await.unwrap();

        assert!(second > first);
    }

    #[tokio::test]
    async fn lifecycle_publishes_active_generation_when_connecting_begins() {
        let event_bus = AppEventBus::default();
        let mut rx = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus, show);

        let generation = lifecycle.begin_connecting().await.unwrap();

        let event = rx.recv().await.unwrap();
        assert!(matches!(
            event,
            AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation: event_generation })
                if event_generation == generation
        ));
    }

    #[tokio::test]
    async fn lifecycle_exposes_show_command_bus_before_runtime_connection() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus, show);

        let bus = lifecycle.current_command_bus().await;
        let result = bus.set_lockout(true).await.unwrap();
        assert!(result.changed);
    }

    #[tokio::test]
    async fn app_lifecycle_initializes_show_command_bus_on_construction() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus, show);

        let result = lifecycle
            .current_command_bus()
            .await
            .set_lockout(true)
            .await
            .unwrap();

        assert!(result.changed);
    }

    #[test]
    fn app_lifecycle_constructs_without_tokio_runtime() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());

        let _lifecycle = AppLifecycle::new(event_bus, show);
    }

    fn discovered_system(host: &str) -> DiscoveredLv1System {
        DiscoveredLv1System {
            identity: Lv1SystemIdentity {
                uuid: None,
                host: Some(host.to_string()),
                address: host.to_string(),
                port: 0,
            },
            latency_ms: Some(1),
            status: DiscoveredLv1Status::Available,
        }
    }

    #[tokio::test]
    async fn disconnected_discovery_metadata_uses_app_lifetime_command_bus() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus, show);
        let bus = lifecycle.current_command_bus().await;

        let result = bus
            .set_discovered_lv1_systems(vec![discovered_system("10.0.0.2")])
            .await
            .unwrap();

        assert!(result.changed);
    }

    #[tokio::test]
    async fn connected_identity_metadata_is_applied_through_app_command_bus() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus, show);
        let command_bus = lifecycle.current_command_bus().await;
        let mut events = lifecycle.event_bus.subscribe();
        let identity = Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };

        lifecycle
            .apply_connected_lv1_metadata(&command_bus, identity.clone(), ReconnectState::default())
            .await
            .expect("connected metadata should apply through the command bus");

        assert!(matches!(
            events.recv().await.unwrap(),
            AppEvent::Show(ShowEvent::StateChanged {
                reason: ShowProjectionReason::ConnectionMetadata,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn failed_connect_metadata_is_applied_through_app_command_bus() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus, show);
        let command_bus = lifecycle.current_command_bus().await;
        let mut events = lifecycle.event_bus.subscribe();
        let identity = Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };

        command_bus
            .set_pending_lv1_identity(Some(identity.clone()))
            .await
            .unwrap();
        command_bus
            .establish_connected_lv1_identity(identity.clone())
            .await
            .unwrap();
        command_bus
            .set_reconnect_state(ReconnectState {
                active: true,
                attempt: 3,
            })
            .await
            .unwrap();

        lifecycle
            .apply_failed_connect_metadata(
                &command_bus,
                ConnectFailureMode::PreserveConnectedIdentity,
            )
            .await
            .expect("failed connect metadata should apply through the command bus");

        assert!(matches!(
            events.recv().await.unwrap(),
            AppEvent::Show(ShowEvent::StateChanged {
                reason: ShowProjectionReason::ConnectionMetadata,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn connect_failure_clears_runtime_targets_after_failed_initial_lv1_state() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus, show);
        let generation = lifecycle.begin_connecting().await.unwrap();
        let command_bus = lifecycle.current_command_bus().await;
        let lv1 = fake_lv1_handle(disconnected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = FadeEngineHandle::new(fade_tx);
        let identity = Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };

        let result = lifecycle
            .finish_connect_transaction_inner(
                mock_app().handle().clone(),
                identity,
                ConnectFailureMode::ClearConnectedIdentity,
                generation,
                command_bus,
                lifecycle.event_bus.clone(),
                lv1,
                fade,
                None,
            )
            .await;

        assert!(matches!(result, Err(message) if message == "LV1 did not connect"));
        assert!(lifecycle.current_lv1().await.is_none());
    }

    #[tokio::test]
    async fn connect_installs_runtime_targets_before_scene_recall_startup() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus.clone(), show);
        let generation = lifecycle.begin_connecting().await.unwrap();
        let command_bus = lifecycle.current_command_bus().await;
        let lv1 = fake_lv1_handle(connected_snapshot());
        let lv1_for_assertion = lv1.clone();
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = FadeEngineHandle::new(fade_tx);
        let identity = Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };
        let (seen_tx, seen_rx) = oneshot::channel();

        let result = lifecycle
            .finish_connect_transaction_inner(
                mock_app().handle().clone(),
                identity,
                ConnectFailureMode::ClearConnectedIdentity,
                generation,
                command_bus,
                event_bus,
                lv1,
                fade,
                Some(Box::new(move |_bus: AppCommandBus| {
                    let seen_tx = seen_tx;
                    tokio::spawn(async move {
                        let (reply, rx) = oneshot::channel();
                        let ok = lv1_for_assertion
                            .send(Lv1Command::GetState { reply })
                            .await
                            .is_ok()
                            && matches!(rx.await, Ok(snapshot) if snapshot.connection == ConnectionStatus::Connected);
                        let _ = seen_tx.send(ok);
                    });
                })),
            )
            .await;

        assert!(result.is_ok());
        assert!(matches!(seen_rx.await, Ok(true)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn connect_completion_logs_lv1_connected_for_ui_log_projection() {
        let captured = CapturedLogEvents::default();
        let logs = captured.0.clone();
        let subscriber = Registry::default().with(captured);
        let _guard = tracing::subscriber::set_default(subscriber);
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus.clone(), show);
        let generation = lifecycle.begin_connecting().await.unwrap();
        let command_bus = lifecycle.current_command_bus().await;
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = FadeEngineHandle::new(fade_tx);

        let result = lifecycle
            .finish_connect_transaction_inner(
                mock_app().handle().clone(),
                Lv1SystemIdentity {
                    uuid: Some("uuid-1".to_string()),
                    host: Some("LV1-FOH".to_string()),
                    address: "192.168.1.35".to_string(),
                    port: 50000,
                },
                ConnectFailureMode::ClearConnectedIdentity,
                generation,
                command_bus,
                event_bus,
                lv1,
                fade,
                None,
            )
            .await;

        assert!(result.is_ok());
        assert!(logs.lock().unwrap().iter().any(|log| {
            log.event.as_deref() == Some("lv1_connected")
                && log.host.as_deref() == Some("LV1-FOH")
                && log.port == Some(50000)
        }));
    }

    #[tokio::test]
    async fn connect_retains_scene_recall_fader_handle() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus.clone(), show);
        let generation = lifecycle.begin_connecting().await.unwrap();
        let command_bus = lifecycle.current_command_bus().await;
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = FadeEngineHandle::new(fade_tx);

        let result = lifecycle
            .finish_connect_transaction_inner(
                mock_app().handle().clone(),
                Lv1SystemIdentity {
                    uuid: Some("uuid-1".to_string()),
                    host: Some("LV1-FOH".to_string()),
                    address: "192.168.1.35".to_string(),
                    port: 50000,
                },
                ConnectFailureMode::ClearConnectedIdentity,
                generation,
                command_bus,
                event_bus,
                lv1,
                fade,
                None,
            )
            .await;

        assert!(result.is_ok());
        assert!(
            lifecycle
                .inner
                .lock()
                .await
                .handles
                .scene_recall_fader
                .is_some()
        );
    }

    #[tokio::test]
    async fn runtime_install_starts_show_scene_list_monitor() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus.clone(), show);
        let generation = lifecycle.begin_connecting().await.unwrap();
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = FadeEngineHandle::new(fade_tx);

        assert!(
            lifecycle
                .install_runtime_transaction(
                    generation,
                    RuntimeHandles::with_runtime_targets(lv1, fade)
                )
                .await
                .is_ok()
        );
        event_bus.publish_lv1(
            generation,
            Lv1Event::SceneListChanged(vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }]),
        );

        let mut saw_scene_config = false;
        for _ in 0..8 {
            if let Ok(AppEvent::Show(ShowEvent::StateChanged { state, .. })) = events.recv().await
                && state
                    .scene_configs
                    .iter()
                    .any(|scene| scene.scene_name == "Intro")
            {
                saw_scene_config = true;
                break;
            }
        }
        assert!(saw_scene_config);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn connect_lv1_system_attempts_selected_identity() {
        let captured = CapturedLogEvents::default();
        let logs = captured.0.clone();
        let subscriber = Registry::default().with(captured);
        let _guard = tracing::subscriber::set_default(subscriber);
        let app = mock_app();
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus, show);

        let result = lifecycle
            .connect_lv1_system(
                app.handle().clone(),
                Lv1SystemIdentity {
                    uuid: None,
                    host: Some("Unreachable".to_string()),
                    address: "127.0.0.1".to_string(),
                    port: 1,
                },
            )
            .await;

        assert!(
            result.is_err(),
            "unreachable selected identity should fail instead of returning a false success"
        );
        let logs = logs.lock().unwrap();
        assert!(
            logs.iter()
                .any(|log| log.event.as_deref() == Some("lv1_connect_requested"))
        );
        assert!(
            logs.iter()
                .any(|log| log.event.as_deref() == Some("lv1_connecting"))
        );
        assert!(
            logs.iter()
                .any(|log| log.event.as_deref() == Some("lv1_connect_failed"))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn attempt_reconnect_uses_stored_connected_identity() {
        let captured = CapturedLogEvents::default();
        let logs = captured.0.clone();
        let subscriber = Registry::default().with(captured);
        let _guard = tracing::subscriber::set_default(subscriber);
        let app = mock_app();
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus, show);
        lifecycle
            .current_command_bus()
            .await
            .establish_connected_lv1_identity(Lv1SystemIdentity {
                uuid: None,
                host: Some("Unreachable".to_string()),
                address: "127.0.0.1".to_string(),
                port: 1,
            })
            .await
            .expect("stored identity should be set");

        let result = lifecycle.attempt_reconnect_lv1(app.handle().clone()).await;

        assert!(
            result.is_err(),
            "unreachable stored identity should fail instead of returning a false success"
        );
        assert!(
            logs.lock()
                .unwrap()
                .iter()
                .any(|log| log.event.as_deref() == Some("lv1_reconnect_failed"))
        );
    }

    #[tokio::test]
    async fn startup_auto_connect_noops_without_stored_identity() {
        let app = mock_app();
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus, show);

        let result = lifecycle
            .startup_auto_connect_lv1(app.handle().clone())
            .await
            .expect("startup without a stored identity should not fail");

        assert!(!result.changed);
    }

    #[tokio::test]
    async fn stale_runtime_install_returns_abortable_handles() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus, show);
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = FadeEngineHandle::new(fade_tx);
        let handles = RuntimeHandles::with_runtime_targets(lv1, fade);

        let rejection = lifecycle
            .install_runtime_transaction(1, handles)
            .await
            .expect_err("stale generation should reject the runtime install");

        let mut handles = rejection.into_handles();
        handles.abort_all();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn disconnect_current_runtime_publishes_active_generation_disconnect() {
        let captured = CapturedLogEvents::default();
        let logs = captured.0.clone();
        let subscriber = Registry::default().with(captured);
        let _guard = tracing::subscriber::set_default(subscriber);
        let event_bus = AppEventBus::default();
        let mut rx = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus, show);

        let generation = lifecycle.begin_connecting().await.unwrap();
        assert!(matches!(
            rx.recv().await.unwrap(),
            AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation: event_generation })
                if event_generation == generation
        ));
        let result = lifecycle.disconnect_current_runtime().await.unwrap();

        assert!(result.changed);
        assert!(matches!(
            rx.recv().await.unwrap(),
            AppEvent::Lv1 { generation: event_generation, event: Lv1Event::Disconnected { .. } }
                if event_generation == generation
        ));
        assert!(matches!(
            rx.recv().await.unwrap(),
            AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation: event_generation })
                if event_generation == generation + 1
        ));
        let logs = logs.lock().unwrap();
        assert!(
            logs.iter()
                .any(|log| log.event.as_deref() == Some("lv1_disconnect_requested"))
        );
        assert!(
            logs.iter()
                .any(|log| log.event.as_deref() == Some("lv1_disconnected"))
        );
    }

    #[tokio::test]
    async fn show_runtime_metadata_monitor_ignores_stale_disconnect_facts() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let command_bus = AppCommandBus::new();
        command_bus.set_show_target(show.clone()).await;
        let (tx, mut rx) = mpsc::unbounded_channel();

        let monitor = spawn_show_runtime_metadata_monitor_with_notifier(
            event_bus.subscribe(),
            command_bus,
            move |notification| {
                tx.send(notification).unwrap();
            },
        );

        event_bus.publish_runtime_generation_changed(9);
        event_bus.publish(AppEvent::Lv1 {
            generation: 8,
            event: Lv1Event::Disconnected {
                reason: "stale disconnect".to_string(),
            },
        });

        assert!(matches!(
            rx.recv().await,
            Some(
                ShowRuntimeMetadataMonitorNotification::RuntimeGenerationChanged { generation: 9 }
            )
        ));
        assert!(matches!(
            rx.recv().await,
            Some(
                ShowRuntimeMetadataMonitorNotification::StaleLv1EventIgnored {
                    generation: 8,
                    active_generation: 9,
                }
            )
        ));

        monitor.abort();
    }

    #[tokio::test]
    async fn show_runtime_metadata_monitor_notifies_on_runtime_generation_and_disconnect() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let command_bus = AppCommandBus::new();
        let (tx, mut rx) = mpsc::unbounded_channel();

        let _monitor = spawn_show_runtime_metadata_monitor_with_notifier(
            event_bus.subscribe(),
            command_bus,
            move |notification| {
                tx.send(notification).unwrap();
            },
        );

        event_bus.publish_runtime_generation_changed(7);
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::Disconnected {
                reason: "link lost".to_string(),
            },
        });

        assert!(matches!(
            rx.recv().await,
            Some(
                ShowRuntimeMetadataMonitorNotification::RuntimeGenerationChanged { generation: 7 }
            )
        ));
        assert!(matches!(
            rx.recv().await,
            Some(ShowRuntimeMetadataMonitorNotification::Lv1Disconnected {
                generation: 7,
                active: true
            })
        ));

        drop(show);
    }

    #[tokio::test]
    async fn show_runtime_metadata_monitor_notifies_on_ignored_stale_events() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let command_bus = AppCommandBus::new();
        let (tx, mut rx) = mpsc::unbounded_channel();

        let _monitor = spawn_show_runtime_metadata_monitor_with_notifier(
            event_bus.subscribe(),
            command_bus,
            move |notification| {
                tx.send(notification).unwrap();
            },
        );

        event_bus.publish_runtime_generation_changed(8);
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::Disconnected {
                reason: "stale".to_string(),
            },
        });

        assert!(matches!(
            rx.recv().await,
            Some(
                ShowRuntimeMetadataMonitorNotification::RuntimeGenerationChanged { generation: 8 }
            )
        ));
        assert!(matches!(
            rx.recv().await,
            Some(
                ShowRuntimeMetadataMonitorNotification::StaleLv1EventIgnored {
                    generation: 7,
                    active_generation: 8
                }
            )
        ));

        drop(show);
    }
}

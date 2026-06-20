//! App runtime lifecycle ownership.

use std::sync::Arc;

use tauri::{AppHandle, Runtime};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::fade::handle::FadeEngineHandle;
use crate::logging::UiLogEvent;
use crate::lv1::actor::spawn_actor;
use crate::lv1::events::Lv1Event;
use crate::lv1::handle::Lv1ActorHandle;
use crate::runtime::commands::AppCommandBus;
use crate::runtime::events::{AppEvent, AppEventBus, RuntimeLifecycleEvent};
use crate::scene_recall::spawn_scene_recall_fader;
use crate::show::commands::ShowCommandResult;
use crate::show::handle::ShowStateHandle;

#[derive(Default)]
pub struct RuntimeHandles {
    pub lv1: Option<Lv1ActorHandle>,
    pub fade: Option<FadeEngineHandle>,
    pub scene_recall_fader: Option<JoinHandle<()>>,
    pub lifecycle_event_monitor: Option<JoinHandle<()>>,
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
        if let Some(handle) = self.lifecycle_event_monitor.take() {
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
    _show_runtime_metadata_monitor: Option<JoinHandle<()>>,
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
        let show_runtime_metadata_monitor = Some(spawn_show_runtime_metadata_monitor(
            event_bus.subscribe(),
            command_bus.clone(),
        ));

        Self {
            inner: Arc::new(Mutex::new(LifecycleInner {
                generation: 0,
                connecting: false,
                frontend_ready: false,
                handles: RuntimeHandles::default(),
                projector: None,
                _show_runtime_metadata_monitor: show_runtime_metadata_monitor,
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

        let (lv1, fade) = match (handles.lv1.clone(), handles.fade.clone()) {
            (Some(lv1), Some(fade)) => (lv1, fade),
            _ => return Err(RuntimeInstallRejection::MissingRuntimeTargets { handles }),
        };

        inner
            .command_bus
            .set_runtime_targets(generation, lv1, fade)
            .await;
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
    ) -> Result<crate::show::commands::ConnectCommandResult, String> {
        let event_bus = self.event_bus.clone();
        let command_bus = self.current_command_bus().await;
        let lv1 = spawn_actor(
            identity.address.clone(),
            identity.port,
            event_bus.clone(),
            generation,
        );
        let fade =
            crate::fade::actor::spawn_engine(command_bus.clone(), event_bus.clone(), generation);
        let handles = RuntimeHandles::with_runtime_targets(lv1.clone(), fade.clone());
        if let Err(rejection) = self.install_runtime_transaction(generation, handles).await {
            let mut handles = rejection.into_handles();
            handles.abort_all();
            return Err("generation is stale".to_string());
        }

        let initial_snapshot = command_bus
            .get_lv1_state()
            .await
            .map_err(|error| error.to_string())?;

        if initial_snapshot.connection != crate::lv1::types::ConnectionStatus::Connected {
            self.clear_runtime_transaction(generation).await;
            let _ = self
                .apply_failed_connect_metadata(&command_bus, failure_mode)
                .await;
            return Err("LV1 did not connect".to_string());
        }

        let reconnect_state = crate::connection_state::ReconnectState::default();
        let connect_result = self
            .apply_connected_lv1_metadata(&command_bus, identity.clone(), reconnect_state)
            .await
            .map_err(|error| error.to_string())?;
        let _ = lv1;
        let _ = fade;
        let _scene_recall_fader = spawn_scene_recall_fader(generation, command_bus, event_bus);
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
        lv1: crate::lv1::handle::Lv1ActorHandle,
        fade: crate::fade::handle::FadeEngineHandle,
        before_scene_recall_start: Option<Box<dyn FnOnce(AppCommandBus) + Send>>,
    ) -> Result<crate::show::commands::ConnectCommandResult, String> {
        let handles = RuntimeHandles::with_runtime_targets(lv1.clone(), fade.clone());

        if self
            .install_runtime_transaction(generation, handles)
            .await
            .is_err()
        {
            return Err("generation is stale".to_string());
        }

        let initial_snapshot = self
            .current_command_bus()
            .await
            .get_lv1_state()
            .await
            .map_err(|error| error.to_string())?;
        if initial_snapshot.connection != crate::lv1::types::ConnectionStatus::Connected {
            self.clear_runtime_transaction(generation).await;
            let _ = self
                .apply_failed_connect_metadata(&command_bus, failure_mode)
                .await;
            return Err("LV1 did not connect".to_string());
        }

        let reconnect_state = crate::connection_state::ReconnectState::default();
        let connect_result = self
            .apply_connected_lv1_metadata(&command_bus, identity.clone(), reconnect_state)
            .await
            .map_err(|error| error.to_string())?;
        if let Some(before_scene_recall_start) = before_scene_recall_start {
            before_scene_recall_start(command_bus.clone());
        }
        let _scene_recall_fader = spawn_scene_recall_fader(generation, command_bus, event_bus);
        let _ = app;
        Ok(connect_result)
    }

    async fn apply_connected_lv1_metadata(
        &self,
        command_bus: &AppCommandBus,
        identity: crate::connection_state::Lv1SystemIdentity,
        reconnect: crate::connection_state::ReconnectState,
    ) -> Result<
        crate::show::commands::ConnectCommandResult,
        crate::runtime::commands::AppCommandError,
    > {
        let pending_result = command_bus.set_pending_lv1_identity(None).await?;
        let connected_result = command_bus
            .establish_connected_lv1_identity(identity)
            .await?;
        let reconnect_result = command_bus.set_reconnect_state(reconnect).await?;
        Ok(crate::show::commands::ConnectCommandResult {
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
        let generation = self.active_generation().await;
        let reason = "Disconnected by user".to_string();
        self.abort_runtime_handles_without_advancing_generation()
            .await;
        self.event_bus.publish(AppEvent::Lv1 {
            generation,
            event: Lv1Event::Disconnected { reason },
        });
        self.clear_runtime_transaction(generation).await;
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

pub fn spawn_lifecycle_event_monitor(
    generation: u64,
    lifecycle: AppLifecycle,
    mut events: tokio::sync::broadcast::Receiver<AppEvent>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(AppEvent::Lv1 {
                    generation: event_generation,
                    event: Lv1Event::Disconnected { .. },
                }) if event_generation == generation => {
                    lifecycle.clear_runtime_transaction(generation).await;
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    crate::runtime::events::log_lagged_subscriber("lifecycle-event-monitor", count);
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
    use crate::fade::handle::FadeEngineHandle;
    use crate::lv1::commands::Lv1Command;
    use crate::show::events::{ShowEvent, ShowProjectionReason};
    use tauri::test::mock_app;
    use tokio::sync::{mpsc, oneshot};

    fn fake_lv1_handle(
        snapshot: crate::lv1::types::Lv1StateSnapshot,
    ) -> crate::lv1::handle::Lv1ActorHandle {
        let (tx, mut rx) = mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if let Lv1Command::GetState { reply } = command {
                    let _ = reply.send(snapshot.clone());
                }
            }
        });
        crate::lv1::handle::Lv1ActorHandle::new(tx)
    }

    fn connected_snapshot() -> crate::lv1::types::Lv1StateSnapshot {
        crate::lv1::types::Lv1StateSnapshot {
            connection: crate::lv1::types::ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![],
            channels: vec![],
        }
    }

    fn disconnected_snapshot() -> crate::lv1::types::Lv1StateSnapshot {
        crate::lv1::types::Lv1StateSnapshot {
            connection: crate::lv1::types::ConnectionStatus::Disconnected,
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
        assert!(matches!(
            lifecycle.current_command_bus().await.get_lv1_state().await,
            Err(crate::runtime::commands::AppCommandError::Lv1Unavailable)
        ));
    }

    #[tokio::test]
    async fn connect_installs_runtime_targets_before_scene_recall_startup() {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        let lifecycle = AppLifecycle::new(event_bus.clone(), show);
        let generation = lifecycle.begin_connecting().await.unwrap();
        let command_bus = lifecycle.current_command_bus().await;
        let lv1 = fake_lv1_handle(connected_snapshot());
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
                Some(Box::new(move |bus: AppCommandBus| {
                    let seen_tx = seen_tx;
                    tokio::spawn(async move {
                        let ok = matches!(bus.get_lv1_state().await, Ok(snapshot) if snapshot.connection == crate::lv1::types::ConnectionStatus::Connected);
                        let _ = seen_tx.send(ok);
                    });
                })),
            )
            .await;

        assert!(result.is_ok());
        assert!(matches!(seen_rx.await, Ok(true)));
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

    #[tokio::test]
    async fn disconnect_current_runtime_publishes_active_generation_disconnect() {
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

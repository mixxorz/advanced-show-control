//! App runtime lifecycle ownership.

use std::sync::Arc;

use tauri::{AppHandle, Runtime};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::fade::handle::FadeEngineHandle;
use crate::logging::UiLogEvent;
use crate::lv1::events::Lv1Event;
use crate::lv1::handle::Lv1ActorHandle;
use crate::runtime::commands::AppCommandBus;
use crate::runtime::events::{AppEvent, AppEventBus, RuntimeLifecycleEvent};
use crate::show::handle::ShowStateHandle;

// Transitional alias while pre-cutover adapters still refer to ActiveCommandBus.
pub type ActiveCommandBus = AppCommandBus;

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

#[derive(Clone)]
pub struct AppLifecycle {
    inner: Arc<Mutex<LifecycleInner>>,
    event_bus: AppEventBus,
    show: ShowStateHandle,
}

impl AppLifecycle {
    pub fn new(event_bus: AppEventBus, show: ShowStateHandle) -> Self {
        let command_bus = AppCommandBus::new();
        command_bus.set_show_target(show.clone());
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

        inner.command_bus.set_runtime_targets(generation, lv1, fade);
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
        inner.command_bus.clear_runtime_targets(generation);
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

    pub async fn current_command_bus(&self) -> AppCommandBus {
        self.inner.lock().await.command_bus.clone()
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

    // Transitional compatibility for pre-cutover command adapters.
    pub fn command_bus_holder(&self) -> AppCommandBus {
        self.inner.blocking_lock().command_bus.clone()
    }

    pub async fn set_command_bus(&self, command_bus: Option<AppCommandBus>) {
        if let Some(command_bus) = command_bus {
            self.inner.lock().await.command_bus = command_bus;
        }
    }

    pub async fn clear_runtime_handles(
        &self,
        _state: &crate::app_state::ShellState,
        generation: u64,
    ) {
        self.clear_runtime_transaction(generation).await;
    }

    pub async fn clear_runtime_handles_with_active_generation(
        &self,
        _state: &crate::app_state::ShellState,
        generation: u64,
    ) {
        self.clear_runtime_transaction(generation).await;
    }

    pub async fn install_runtime_handles(
        &self,
        _state: &crate::app_state::ShellState,
        generation: u64,
        next: crate::app_state::RuntimeHandles,
    ) -> Result<(), crate::app_state::RuntimeHandles> {
        if self.active_generation().await != generation {
            return Err(next);
        }
        if let Some(command_bus) = next.command_bus.clone() {
            self.set_command_bus(Some(command_bus)).await;
        }
        Ok(())
    }

    pub async fn abort_current_runtime_for_shell(&self, state: &crate::app_state::ShellState) {
        state
            .abort_current_runtime(&self.command_bus_holder())
            .await;
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
    mut events: tokio::sync::broadcast::Receiver<AppEvent>,
    command_bus: AppCommandBus,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut active_generation = 0;
        loop {
            match events.recv().await {
                Ok(AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged {
                    generation,
                })) => {
                    active_generation = generation;
                }
                Ok(AppEvent::Lv1 { generation, event }) if generation == active_generation => {
                    if let Lv1Event::Disconnected { reason } = event {
                        let _ = command_bus.handle_runtime_disconnected(reason).await;
                    }
                }
                Ok(AppEvent::Lv1 { .. }) => {}
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
    _state: crate::app_state::ShellState,
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
}

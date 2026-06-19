//! App runtime lifecycle ownership.
//!
//! This module will replace the temporary ActiveCommandBus holder and own
//! runtime task handles, generation, command bus installation, and projector startup.

use crate::runtime::commands::AppCommandBus;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
pub struct ActiveCommandBus(pub Arc<Mutex<Option<AppCommandBus>>>);

impl ActiveCommandBus {
    pub async fn set(&self, command_bus: Option<AppCommandBus>) {
        *self.0.lock().await = command_bus;
    }

    pub async fn current(&self) -> Option<AppCommandBus> {
        self.0.lock().await.clone()
    }
}

#[derive(Clone, Default)]
pub struct AppLifecycle {
    command_bus: ActiveCommandBus,
}

impl AppLifecycle {
    pub fn command_bus_holder(&self) -> ActiveCommandBus {
        self.command_bus.clone()
    }

    pub async fn set_command_bus(&self, command_bus: Option<AppCommandBus>) {
        self.command_bus.set(command_bus).await;
    }

    pub async fn current_command_bus(&self) -> Option<AppCommandBus> {
        self.command_bus.current().await
    }

    pub async fn clear_runtime_handles(
        &self,
        state: &crate::app_state::ShellState,
        generation: u64,
    ) {
        state
            .clear_runtime_handles(generation, &self.command_bus)
            .await;
    }

    pub async fn abort_current_runtime(&self, state: &crate::app_state::ShellState) {
        state.abort_current_runtime(&self.command_bus).await;
    }

    pub async fn clear_runtime_handles_with_active_generation(
        &self,
        state: &crate::app_state::ShellState,
        generation: u64,
    ) {
        state
            .clear_runtime_handles_with_active_generation(generation, &self.command_bus)
            .await;
    }

    pub async fn install_runtime_handles(
        &self,
        state: &crate::app_state::ShellState,
        generation: u64,
        next: crate::app_state::RuntimeHandles,
    ) -> Result<(), crate::app_state::RuntimeHandles> {
        state
            .install_runtime_handles(generation, next, &self.command_bus)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::events::AppEventBus;

    #[tokio::test]
    async fn active_command_bus_tracks_current_bus() {
        let holder = ActiveCommandBus::default();
        assert!(holder.current().await.is_none());

        let command_bus = AppCommandBus::new(AppEventBus::default());
        holder.set(Some(command_bus.clone())).await;
        assert!(holder.current().await.is_some());

        holder.set(None).await;
        assert!(holder.current().await.is_none());
    }

    #[tokio::test]
    async fn app_lifecycle_exposes_current_command_bus() {
        let lifecycle = AppLifecycle::default();
        assert!(lifecycle.current_command_bus().await.is_none());

        let command_bus = AppCommandBus::new(AppEventBus::default());
        lifecycle.set_command_bus(Some(command_bus)).await;
        assert!(lifecycle.current_command_bus().await.is_some());
    }

    #[tokio::test]
    async fn app_lifecycle_command_bus_holder_is_shared() {
        let lifecycle = AppLifecycle::default();
        let holder = lifecycle.command_bus_holder();

        let command_bus = AppCommandBus::new(AppEventBus::default());
        holder.set(Some(command_bus)).await;

        assert!(lifecycle.current_command_bus().await.is_some());
    }

    #[tokio::test]
    async fn lifecycle_installs_command_bus_with_runtime_handles() {
        let lifecycle = AppLifecycle::default();
        let state = crate::app_state::ShellState::default();
        let (generation, _) = state.disconnect().await;
        let command_bus = AppCommandBus::new(AppEventBus::default());

        assert!(
            lifecycle
                .install_runtime_handles(
                    &state,
                    generation,
                    crate::app_state::RuntimeHandles {
                        command_bus: Some(command_bus),
                        ..Default::default()
                    },
                )
                .await
                .is_ok()
        );

        assert!(lifecycle.current_command_bus().await.is_some());
    }

    #[tokio::test]
    async fn lifecycle_clear_runtime_handles_clears_current_bus() {
        let lifecycle = AppLifecycle::default();
        let state = crate::app_state::ShellState::default();
        let (generation, _) = state.disconnect().await;
        let command_bus = AppCommandBus::new(AppEventBus::default());

        assert!(
            lifecycle
                .install_runtime_handles(
                    &state,
                    generation,
                    crate::app_state::RuntimeHandles {
                        command_bus: Some(command_bus),
                        ..Default::default()
                    },
                )
                .await
                .is_ok()
        );

        lifecycle.clear_runtime_handles(&state, generation).await;

        assert!(lifecycle.current_command_bus().await.is_none());
    }
}

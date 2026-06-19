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
}

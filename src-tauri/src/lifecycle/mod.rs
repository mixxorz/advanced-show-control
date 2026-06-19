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
}

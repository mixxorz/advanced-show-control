//! Feature-gated setup and observation used only by the hardware smoke executable.

use crate::lifecycle::AppLifecycle;
use crate::lv1::{Lv1Command, Lv1Connection};

#[derive(Clone)]
pub struct DebugRuntimeCommands {
    lifecycle: AppLifecycle,
}

impl DebugRuntimeCommands {
    pub(crate) fn new(lifecycle: AppLifecycle) -> Self {
        Self { lifecycle }
    }

    /// @cc [owner:mixxorz,label:debug;safety] debug-gain-write-awaits-current-lv1
    /// This debug-only setup MUST bind the current LV1 endpoint to its generation, fence mailbox
    /// admission and the reply against generation changes, and await the actor write acknowledgement.
    pub async fn set_channel_gain(
        &self,
        group: i32,
        channel: i32,
        gain_db: f64,
    ) -> Result<(), String> {
        let lv1 = self.current_lv1_connection().await?;
        lv1.request(|reply| Lv1Command::SetGain {
            group,
            channel,
            gain_db,
            reply: Some(reply),
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
    }

    /// @cc [owner:mixxorz,label:debug;safety] raw-recall-remains-debug-setup-only
    /// This debug-only setup MUST remain outside production commands and may bypass Scenes recall
    /// policy only to establish deterministic smoke preconditions, with generation fencing intact.
    pub async fn recall_lv1_scene(&self, scene_index: i32) -> Result<(), String> {
        let lv1 = self.current_lv1_connection().await?;
        lv1.request(|reply| Lv1Command::RecallScene {
            scene_index,
            reply: Some(reply),
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
        .map(|_| ())
    }

    /// @cc [owner:mixxorz,label:debug] debug-gain-read-uses-actor-snapshot
    /// This observation MUST request fresh state through a generation-bound LV1 connection and fail
    /// when the endpoint, reply, or requested channel is unavailable.
    pub async fn channel_gain(&self, group: i32, channel: i32) -> Result<f64, String> {
        let lv1 = self.current_lv1_connection().await?;
        let snapshot = lv1
            .request(|reply| Lv1Command::GetState { reply })
            .await
            .map_err(|error| error.to_string())?;
        snapshot
            .channels
            .iter()
            .find(|entry| entry.group == group && entry.channel == channel)
            .map(|entry| entry.gain_db)
            .ok_or_else(|| format!("channel {group}:{channel} unavailable"))
    }

    async fn current_lv1_connection(&self) -> Result<Lv1Connection, String> {
        let authority = self.lifecycle.current_runtime_generation().await;
        let (generation, lv1) = self
            .lifecycle
            .runtime_snapshot_source()
            .connected_lv1()
            .await
            .ok_or_else(|| "LV1 is unavailable".to_string())?;
        Ok(Lv1Connection::new(lv1, authority, generation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::build_actor;
    use crate::runtime::events::AppEventBus;
    use crate::runtime::generation::RuntimeGeneration;

    #[tokio::test]
    async fn stale_generation_prevents_debug_gain_dispatch() {
        let connection = stale_connection().await;
        let error = connection
            .request(|reply| Lv1Command::SetGain {
                group: 0,
                channel: 1,
                gain_db: -10.0,
                reply: Some(reply),
            })
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "generation is stale");
    }

    #[tokio::test]
    async fn stale_generation_prevents_debug_raw_recall_dispatch() {
        let connection = stale_connection().await;
        let error = connection
            .request(|reply| Lv1Command::RecallScene {
                scene_index: 1,
                reply: Some(reply),
            })
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "generation is stale");
    }

    #[tokio::test]
    async fn stale_generation_prevents_debug_gain_read_dispatch() {
        let connection = stale_connection().await;
        let error = connection
            .request(|reply| Lv1Command::GetState { reply })
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "generation is stale");
    }

    async fn stale_connection() -> Lv1Connection {
        let authority = RuntimeGeneration::new();
        let generation = authority.current().await;
        let (lv1, _task) = build_actor(
            "127.0.0.1".to_string(),
            9,
            AppEventBus::default(),
            generation,
        );
        let connection = Lv1Connection::new(lv1, authority.clone(), generation);
        authority.advance().await;
        connection
    }
}

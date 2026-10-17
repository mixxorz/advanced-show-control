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
    use crate::lv1::{
        ChannelInfo, ConnectionStatus, Lv1StateSnapshot, RecallSceneDispatch, test_actor_handle,
    };
    use std::future::{Future, poll_fn};
    use std::pin::Pin;
    use std::task::Poll;
    use std::time::Duration;
    use tokio::sync::mpsc;

    #[derive(Clone, Copy, Debug)]
    enum DebugOperation {
        SetGain,
        RecallScene,
        ReadGain,
    }

    impl DebugOperation {
        async fn run(self, commands: &DebugRuntimeCommands) -> Result<Option<f64>, String> {
            match self {
                Self::SetGain => commands.set_channel_gain(2, 3, -10.0).await.map(|()| None),
                Self::RecallScene => commands.recall_lv1_scene(7).await.map(|()| None),
                Self::ReadGain => commands.channel_gain(2, 3).await.map(Some),
            }
        }

        fn acknowledge(self, command: Lv1Command) {
            match (self, command) {
                (
                    Self::SetGain,
                    Lv1Command::SetGain {
                        group,
                        channel,
                        gain_db,
                        reply: Some(reply),
                    },
                ) => {
                    assert_eq!((group, channel, gain_db), (2, 3, -10.0));
                    reply.send(Ok(())).unwrap();
                }
                (
                    Self::RecallScene,
                    Lv1Command::RecallScene {
                        scene_index,
                        reply: Some(reply),
                    },
                ) => {
                    assert_eq!(scene_index, 7);
                    reply
                        .send(Ok(RecallSceneDispatch {
                            scene_observation_sequence: 11,
                        }))
                        .unwrap();
                }
                (Self::ReadGain, Lv1Command::GetState { reply }) => {
                    reply.send(snapshot()).unwrap();
                }
                _ => panic!("debug operation should send its acknowledged LV1 command"),
            }
        }
    }

    fn snapshot() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: None,
            channels: vec![ChannelInfo {
                group: 2,
                channel: 3,
                name: "Test channel".into(),
                gain_db: -10.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            ping_sequence: 0,
        }
    }

    async fn fixture() -> (
        DebugRuntimeCommands,
        mpsc::Sender<Lv1Command>,
        mpsc::Receiver<Lv1Command>,
    ) {
        let lifecycle = AppLifecycle::default();
        let (sender, receiver) = mpsc::channel(1);
        lifecycle
            .install_lv1_for_test(test_actor_handle(sender.clone()))
            .await;
        (DebugRuntimeCommands::new(lifecycle), sender, receiver)
    }

    async fn poll_pending<T>(mut future: Pin<&mut impl Future<Output = T>>) {
        poll_fn(|cx| {
            assert!(future.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
    }

    #[tokio::test]
    async fn debug_commands_reject_revocation_during_blocked_mailbox_admission() {
        for operation in [
            DebugOperation::SetGain,
            DebugOperation::RecallScene,
            DebugOperation::ReadGain,
        ] {
            let (debug, sender, mut receiver) = fixture().await;
            sender
                .send(Lv1Command::Flush { reply: None })
                .await
                .unwrap();
            let request = operation.run(&debug);
            tokio::pin!(request);
            // Endpoint lookup has no contended locks; the first poll reaches the full mailbox.
            poll_pending(request.as_mut()).await;

            debug.lifecycle.begin_connecting().await.unwrap();
            assert!(matches!(
                receiver.recv().await,
                Some(Lv1Command::Flush { reply: None })
            ));
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(1), request)
                    .await
                    .expect("revoked debug request should finish"),
                Err("generation is stale".into()),
                "{operation:?} must recheck the generation after capacity becomes available"
            );
            assert!(matches!(
                receiver.try_recv(),
                Err(mpsc::error::TryRecvError::Empty)
            ));
        }
    }

    #[tokio::test]
    async fn debug_commands_reject_successful_replies_after_generation_revocation() {
        for operation in [
            DebugOperation::SetGain,
            DebugOperation::RecallScene,
            DebugOperation::ReadGain,
        ] {
            let (debug, _sender, mut receiver) = fixture().await;
            let request = operation.run(&debug);
            tokio::pin!(request);
            poll_pending(request.as_mut()).await;
            let command = receiver.recv().await.unwrap();

            debug.lifecycle.begin_connecting().await.unwrap();
            operation.acknowledge(command);
            assert_eq!(
                request.await,
                Err("generation is stale".into()),
                "{operation:?} must not return a stale acknowledgement or value"
            );
        }
    }

    #[tokio::test]
    async fn debug_commands_require_an_installed_lv1_endpoint() {
        let debug = DebugRuntimeCommands::new(AppLifecycle::default());
        for operation in [
            DebugOperation::SetGain,
            DebugOperation::RecallScene,
            DebugOperation::ReadGain,
        ] {
            assert_eq!(
                operation.run(&debug).await,
                Err("LV1 is unavailable".into())
            );
        }
    }

    #[tokio::test]
    async fn debug_gain_read_uses_fresh_snapshot_and_reports_missing_channels() {
        let (debug, _sender, mut receiver) = fixture().await;
        for (group, channel, expected) in [
            (2, 3, Ok(-10.0)),
            (2, 4, Err("channel 2:4 unavailable".into())),
        ] {
            let request = debug.channel_gain(group, channel);
            tokio::pin!(request);
            poll_pending(request.as_mut()).await;
            let Some(Lv1Command::GetState { reply }) = receiver.recv().await else {
                panic!("gain reads should request a fresh actor snapshot");
            };
            reply.send(snapshot()).unwrap();
            assert_eq!(request.await, expected);
        }
    }
}

use tokio::sync::mpsc;

use super::commands::Lv1Command;
use super::events::Lv1ActorError;

/// A cloneable handle to the LV1 actor. Use this to send commands.
#[derive(Clone)]
pub struct Lv1ActorHandle {
    tx: mpsc::Sender<Lv1Command>,
}

impl Lv1ActorHandle {
    pub(super) fn new(tx: mpsc::Sender<Lv1Command>) -> Self {
        Self { tx }
    }

    pub async fn send(&self, command: Lv1Command) -> Result<(), Lv1ActorError> {
        self.tx
            .send(command)
            .await
            .map_err(|_| Lv1ActorError::CommandChannelClosed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn handle_sends_pan_family_commands() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(3);
        let handle = Lv1ActorHandle::new(tx);

        let (reply, pan_rx) = oneshot::channel();
        let pan = tokio::spawn({
            let handle = handle.clone();
            async move {
                handle
                    .send(Lv1Command::SetPan {
                        group: 1,
                        channel: 2,
                        value: -0.5,
                        reply: Some(reply),
                    })
                    .await
            }
        });
        if let Some(Lv1Command::SetPan {
            group,
            channel,
            value,
            reply,
        }) = rx.recv().await
        {
            assert_eq!((group, channel, value), (1, 2, -0.5));
            let _ = reply.unwrap().send(Ok(()));
        } else {
            panic!("expected SetPan command");
        }
        assert_eq!(pan.await.unwrap(), Ok(()));
        assert_eq!(pan_rx.await.unwrap(), Ok(()));

        let (reply, balance_rx) = oneshot::channel();
        let balance = tokio::spawn({
            let handle = handle.clone();
            async move {
                handle
                    .send(Lv1Command::SetBalance {
                        group: 3,
                        channel: 4,
                        value: 0.25,
                        reply: Some(reply),
                    })
                    .await
            }
        });
        if let Some(Lv1Command::SetBalance {
            group,
            channel,
            value,
            reply,
        }) = rx.recv().await
        {
            assert_eq!((group, channel, value), (3, 4, 0.25));
            let _ = reply.unwrap().send(Ok(()));
        } else {
            panic!("expected SetBalance command");
        }
        assert_eq!(balance.await.unwrap(), Ok(()));
        assert_eq!(balance_rx.await.unwrap(), Ok(()));

        let (reply, width_rx) = oneshot::channel();
        let width = tokio::spawn({
            let handle = handle.clone();
            async move {
                handle
                    .send(Lv1Command::SetWidth {
                        group: 5,
                        channel: 6,
                        value: 0.75,
                        reply: Some(reply),
                    })
                    .await
            }
        });
        if let Some(Lv1Command::SetWidth {
            group,
            channel,
            value,
            reply,
        }) = rx.recv().await
        {
            assert_eq!((group, channel, value), (5, 6, 0.75));
            let _ = reply.unwrap().send(Ok(()));
        } else {
            panic!("expected SetWidth command");
        }
        assert_eq!(width.await.unwrap(), Ok(()));
        assert_eq!(width_rx.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn handle_sends_write_batch_without_reply() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let handle = Lv1ActorHandle::new(tx);
        let writes = vec![crate::lv1::commands::Lv1ParameterWrite {
            group: 0,
            channel: 1,
            parameter: crate::lv1::commands::Lv1WriteParameter::FaderDb,
            value: -18.0,
        }];

        assert_eq!(
            handle.send(Lv1Command::WriteBatch(writes.clone())).await,
            Ok(())
        );

        match rx.recv().await {
            Some(Lv1Command::WriteBatch(received)) => assert_eq!(received, writes),
            Some(_) => panic!("expected WriteBatch, got some other command"),
            None => panic!("expected WriteBatch, got None"),
        }
    }

    #[tokio::test]
    async fn handle_sends_recall_scene_command() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let handle = Lv1ActorHandle::new(tx);

        let (reply, recall_rx) = oneshot::channel();
        let recall = tokio::spawn(async move {
            handle
                .send(Lv1Command::RecallScene {
                    scene_index: 4,
                    reply: Some(reply),
                })
                .await
        });

        if let Some(Lv1Command::RecallScene { scene_index, reply }) = rx.recv().await {
            assert_eq!(scene_index, 4);
            reply.unwrap().send(Ok(())).unwrap();
        } else {
            panic!("expected RecallScene command");
        }

        assert!(recall.await.unwrap().is_ok());
        assert_eq!(recall_rx.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn handle_sends_recall_scene_without_reply() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let handle = Lv1ActorHandle::new(tx);

        assert_eq!(
            handle
                .send(Lv1Command::RecallScene {
                    scene_index: 4,
                    reply: None,
                })
                .await,
            Ok(())
        );

        if let Some(Lv1Command::RecallScene { scene_index, reply }) = rx.recv().await {
            assert_eq!(scene_index, 4);
            assert!(reply.is_none());
        } else {
            panic!("expected RecallScene command");
        }
    }
}

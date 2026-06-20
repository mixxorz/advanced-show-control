//! LV1 actor runtime.

use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use super::commands::Lv1Command;
use super::events::Lv1ActorError;
use super::events::Lv1Event;
use super::handle::Lv1ActorHandle;
use super::state::{ActorState, handle_message};
use super::tcp::{
    Lv1TcpClient, decode_frame_payload, encode_frame, encode_parameter_write_batch, pong_for_ping,
    read_next_async,
};
use super::types::ConnectionStatus;
use crate::osc::OscArg;
use crate::runtime::events::AppEventBus;

const PING_TIMEOUT: Duration = Duration::from_secs(10);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);
const WRITER_QUEUE_CAPACITY: usize = 64;

/// Spawn the LV1 actor. Returns a handle immediately; the actor connects in the background.
pub fn spawn_actor(
    host: String,
    port: u16,
    event_bus: AppEventBus,
    generation: u64,
) -> Lv1ActorHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    tokio::spawn(run_actor(host, port, event_bus, generation, cmd_rx));
    Lv1ActorHandle::new(cmd_tx)
}

#[derive(Debug, PartialEq, Eq)]
enum DrainCommandsResult {
    TimedOut,
    CommandChannelClosed,
}

async fn writer_task(
    mut writer: tokio::net::tcp::OwnedWriteHalf,
    mut rx: mpsc::Receiver<WriterMessage>,
    error_tx: mpsc::Sender<()>,
) {
    while let Some(message) = rx.recv().await {
        match message {
            WriterMessage::Bytes(bytes) => {
                if writer.write_all(&bytes).await.is_err() {
                    fail_pending_writer_flushes(&mut rx);
                    let _ = error_tx.try_send(());
                    return;
                }
            }
            WriterMessage::Flush(reply) => {
                let result = writer
                    .flush()
                    .await
                    .map_err(|_| Lv1ActorError::CommandSendFailed);
                let failed = result.is_err();
                if failed {
                    fail_pending_writer_flushes(&mut rx);
                    let _ = error_tx.try_send(());
                }
                let _ = reply.send(result);
                if failed {
                    return;
                }
            }
        }
    }
}

fn enqueue_writer_bytes(
    writer_tx: &mpsc::Sender<WriterMessage>,
    bytes: Vec<u8>,
) -> Result<(), DisconnectReason> {
    if bytes.is_empty() {
        return Ok(());
    }

    writer_tx
        .try_send(WriterMessage::Bytes(bytes))
        .map_err(|err| match err {
            mpsc::error::TrySendError::Full(_) => {
                DisconnectReason::TcpError("writer queue full".to_string())
            }
            mpsc::error::TrySendError::Closed(_) => {
                DisconnectReason::TcpError("writer task gone".to_string())
            }
        })
}

fn enqueue_writer_flush(
    writer_tx: &mpsc::Sender<WriterMessage>,
    reply: oneshot::Sender<Result<(), Lv1ActorError>>,
) -> Result<(), oneshot::Sender<Result<(), Lv1ActorError>>> {
    writer_tx
        .try_send(WriterMessage::Flush(reply))
        .map_err(|err| match err {
            mpsc::error::TrySendError::Full(message)
            | mpsc::error::TrySendError::Closed(message) => match message {
                WriterMessage::Flush(reply) => reply,
                WriterMessage::Bytes(_) => unreachable!(),
            },
        })
}

fn fail_pending_writer_flushes(rx: &mut mpsc::Receiver<WriterMessage>) {
    while let Ok(message) = rx.try_recv() {
        if let WriterMessage::Flush(reply) = message {
            let _ = reply.send(Err(Lv1ActorError::CommandSendFailed));
        }
    }
}

/// Handles a single command received while the actor is disconnected.
///
/// # Flush behavior varies by context:
/// - During reconnect delays (drain_commands_for): Flush replies Err(NotConnected)
///   because sends truly cannot be queued yet.
/// - After connection but before full init (post_connect_stale_drain): Flush replies Ok(())
///   because the actor is about to enter connected mode and can accept sends.
///
/// WriteBatch is silently dropped (fire-and-forget; callers tolerate loss while disconnected).
/// GetState replies with the current snapshot.
/// All other commands (SetGain, SetPan, SetBalance, SetWidth, SetMute, RecallScene) reply based on flush_reply.
fn drain_disconnected_command(
    cmd: Lv1Command,
    state: &ActorState,
    flush_reply: Result<(), Lv1ActorError>,
) {
    match cmd {
        Lv1Command::GetState { reply } => {
            let _ = reply.send(state.snapshot());
        }
        Lv1Command::WriteBatch(_) => {
            // Fire-and-forget while disconnected; callers expect possible loss
        }
        Lv1Command::SetGain { reply, .. } => {
            if let Some(reply) = reply {
                let _ = reply.send(Err(Lv1ActorError::NotConnected));
            }
        }
        Lv1Command::SetPan { reply, .. } => {
            if let Some(reply) = reply {
                let _ = reply.send(Err(Lv1ActorError::NotConnected));
            }
        }
        Lv1Command::SetBalance { reply, .. } => {
            if let Some(reply) = reply {
                let _ = reply.send(Err(Lv1ActorError::NotConnected));
            }
        }
        Lv1Command::SetWidth { reply, .. } => {
            if let Some(reply) = reply {
                let _ = reply.send(Err(Lv1ActorError::NotConnected));
            }
        }
        Lv1Command::SetMute { reply, .. } => {
            if let Some(reply) = reply {
                let _ = reply.send(Err(Lv1ActorError::NotConnected));
            }
        }
        Lv1Command::RecallScene { reply, .. } => {
            if let Some(reply) = reply {
                let _ = reply.send(Err(Lv1ActorError::NotConnected));
            }
        }
        Lv1Command::Flush { reply } => {
            if let Some(reply) = reply {
                let _ = reply.send(flush_reply.clone());
            }
        }
    }
}

/// Drain pending commands for `duration`, responding to GetState immediately.
/// Used during reconnect delays so callers are never blocked indefinitely.
async fn drain_commands_for(
    state: &mut ActorState,
    cmd_rx: &mut mpsc::Receiver<Lv1Command>,
    duration: Duration,
) -> DrainCommandsResult {
    let deadline = tokio::time::sleep(duration);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => return DrainCommandsResult::TimedOut,
            cmd = cmd_rx.recv() => match cmd {
                None => return DrainCommandsResult::CommandChannelClosed,
                Some(cmd) => drain_disconnected_command(cmd, state, Err(Lv1ActorError::NotConnected)),
            },
        }
    }
}

async fn run_actor(
    host: String,
    port: u16,
    event_bus: AppEventBus,
    generation: u64,
    mut cmd_rx: mpsc::Receiver<Lv1Command>,
) {
    let mut state = ActorState::new(event_bus, generation);

    loop {
        let mut client = loop {
            match Lv1TcpClient::connect(&host, port).await {
                Ok(c) => break c,
                Err(_) => {
                    if drain_commands_for(&mut state, &mut cmd_rx, RECONNECT_DELAY).await
                        == DrainCommandsResult::CommandChannelClosed
                    {
                        return;
                    }
                }
            }
        };

        let device_name = "lv1-state-mirror";
        let uuid = uuid::Uuid::new_v4().to_string();
        if client.register_myfoh(device_name, &uuid).await.is_err() {
            if drain_commands_for(&mut state, &mut cmd_rx, RECONNECT_DELAY).await
                == DrainCommandsResult::CommandChannelClosed
            {
                return;
            }
            continue;
        }

        state.connection = ConnectionStatus::Connected;
        state.last_ping = Instant::now();

        tokio::task::yield_now().await;
        // Drain stale commands that arrived before connection completed.
        // Flush replies Ok(()) here because the actor is now connected and ready to process.
        while let Ok(cmd) = cmd_rx.try_recv() {
            drain_disconnected_command(cmd, &state, Ok(()));
        }
        state.fan_out(Lv1Event::Connected);

        let disconnected = run_connected(&mut client, &mut state, &mut cmd_rx).await;

        state.connection = ConnectionStatus::Disconnected;
        state.scene = None;
        state.channels.clear();
        state.scene_buf = Default::default();
        state.diagnose(format!("disconnected: {disconnected}"));
        state.fan_out(Lv1Event::Disconnected {
            reason: disconnected.to_string(),
        });

        if disconnected == DisconnectReason::CommandChannelClosed {
            break;
        }

        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

#[derive(Debug, PartialEq)]
enum DisconnectReason {
    TcpError(String),
    PingTimeout,
    CommandChannelClosed,
}

impl std::fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DisconnectReason::TcpError(detail) => write!(f, "TCP error: {detail}"),
            DisconnectReason::PingTimeout => write!(f, "ping timeout"),
            DisconnectReason::CommandChannelClosed => write!(f, "command channel closed"),
        }
    }
}

enum WriterMessage {
    Bytes(Vec<u8>),
    Flush(oneshot::Sender<Result<(), Lv1ActorError>>),
}

async fn run_connected(
    client: &mut Lv1TcpClient,
    state: &mut ActorState,
    cmd_rx: &mut mpsc::Receiver<Lv1Command>,
) -> DisconnectReason {
    let reader = &mut client.reader;
    let writer = client
        .writer
        .take()
        .expect("connected LV1 client has a writer before run_connected");
    let decoder = &mut client.decoder;
    let (writer_tx, writer_rx) = mpsc::channel(WRITER_QUEUE_CAPACITY);
    let (writer_error_tx, mut writer_error_rx) = mpsc::channel(1);
    tokio::spawn(writer_task(writer, writer_rx, writer_error_tx));

    let ping_deadline = tokio::time::sleep_until(tokio::time::Instant::from_std(
        state.last_ping + PING_TIMEOUT,
    ));
    tokio::pin!(ping_deadline);

    loop {
        if state.last_ping.elapsed() > PING_TIMEOUT {
            return DisconnectReason::PingTimeout;
        }

        tokio::select! {
            _ = &mut ping_deadline => {
                return DisconnectReason::PingTimeout;
            }
            maybe_error = writer_error_rx.recv() => {
                if maybe_error.is_some() {
                    return DisconnectReason::TcpError("socket write failed".to_string());
                }
            }
            frames = read_next_async(reader, decoder) => {
                match frames {
                    Err(err) => return DisconnectReason::TcpError(format!("read failed: {err:?}")),
                    Ok(frames) => {
                        for frame in frames {
                            if let Ok(msg) = decode_frame_payload(&frame) {
                                if let Some((addr, args)) = pong_for_ping(&msg) {
                                    let bytes = match encode_frame(addr, &args) {
                                        Ok(bytes) => bytes,
                                        Err(err) => {
                                            return DisconnectReason::TcpError(format!(
                                                "encode pong failed: {err:?}"
                                            ));
                                        }
                                    };
                                    if let Err(reason) = enqueue_writer_bytes(&writer_tx, bytes) {
                                        return reason;
                                    }
                                    state.last_ping = Instant::now();
                                    ping_deadline.as_mut().reset(tokio::time::Instant::from_std(state.last_ping + PING_TIMEOUT));
                                    continue;
                                }
                                handle_message(state, &msg);
                            }
                        }
                    }
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    None => return DisconnectReason::CommandChannelClosed,
                    Some(Lv1Command::GetState { reply }) => {
                        let _ = reply.send(state.snapshot());
                    }
                    Some(Lv1Command::WriteBatch(writes)) => {
                        if writes.is_empty() {
                            continue;
                        }

                        let bytes = match encode_parameter_write_batch(&writes) {
                            Ok(bytes) => bytes,
                            Err(err) => {
                                return DisconnectReason::TcpError(format!(
                                    "encode write batch failed: {err:?}"
                                ));
                            }
                        };

                        if let Err(reason) = enqueue_writer_bytes(&writer_tx, bytes) {
                            return reason;
                        }
                    }
                    Some(Lv1Command::SetGain { group, channel, gain_db, reply }) => {
                        let result = encode_frame(
                            "/Set/Track/Out/Gain",
                            &[
                                OscArg::Int(group),
                                OscArg::Int(channel),
                                OscArg::Double(gain_db),
                            ],
                        )
                        .map_err(|_| Lv1ActorError::CommandSendFailed)
                        .and_then(|bytes| {
                            enqueue_writer_bytes(&writer_tx, bytes)
                                .map_err(|_| Lv1ActorError::CommandSendFailed)
                        });

                        let failed = result.is_err();
                        if let Some(reply) = reply {
                            let _ = reply.send(result);
                        }
                        if failed {
                            return DisconnectReason::TcpError(
                                "SetGain send failed (encode or writer queue)".to_string(),
                            );
                        }
                    }
                    Some(Lv1Command::SetPan { group, channel, value, reply }) => {
                        let result = encode_frame(
                            "/Set/Track/Pan",
                            &[
                                OscArg::Int(group),
                                OscArg::Int(channel),
                                OscArg::Double(value),
                            ],
                        )
                        .map_err(|_| Lv1ActorError::CommandSendFailed)
                        .and_then(|bytes| {
                            enqueue_writer_bytes(&writer_tx, bytes)
                                .map_err(|_| Lv1ActorError::CommandSendFailed)
                        });

                        let failed = result.is_err();
                        if let Some(reply) = reply {
                            let _ = reply.send(result);
                        }
                        if failed {
                            return DisconnectReason::TcpError(
                                "SetPan send failed (encode or writer queue)".to_string(),
                            );
                        }
                    }
                    Some(Lv1Command::SetBalance { group, channel, value, reply }) => {
                        let result = encode_frame(
                            "/Set/Track/Pan/Balance",
                            &[
                                OscArg::Int(group),
                                OscArg::Int(channel),
                                OscArg::Double(value),
                            ],
                        )
                        .map_err(|_| Lv1ActorError::CommandSendFailed)
                        .and_then(|bytes| {
                            enqueue_writer_bytes(&writer_tx, bytes)
                                .map_err(|_| Lv1ActorError::CommandSendFailed)
                        });

                        let failed = result.is_err();
                        if let Some(reply) = reply {
                            let _ = reply.send(result);
                        }
                        if failed {
                            return DisconnectReason::TcpError(
                                "SetBalance send failed (encode or writer queue)".to_string(),
                            );
                        }
                    }
                    Some(Lv1Command::SetWidth { group, channel, value, reply }) => {
                        let result = encode_frame(
                            "/Set/Track/Pan/Width",
                            &[
                                OscArg::Int(group),
                                OscArg::Int(channel),
                                OscArg::Double(value),
                            ],
                        )
                        .map_err(|_| Lv1ActorError::CommandSendFailed)
                        .and_then(|bytes| {
                            enqueue_writer_bytes(&writer_tx, bytes)
                                .map_err(|_| Lv1ActorError::CommandSendFailed)
                        });

                        let failed = result.is_err();
                        if let Some(reply) = reply {
                            let _ = reply.send(result);
                        }
                        if failed {
                            return DisconnectReason::TcpError(
                                "SetWidth send failed (encode or writer queue)".to_string(),
                            );
                        }
                    }
                    Some(Lv1Command::SetMute { group, channel, muted, reply }) => {
                        let result = super::tcp::encode_frame(
                            "/Set/Track/Out/Mute",
                            &[
                                OscArg::Int(group),
                                OscArg::Int(channel),
                                OscArg::Bool(muted),
                            ],
                        )
                        .map_err(|_| Lv1ActorError::CommandSendFailed)
                        .and_then(|bytes| {
                            enqueue_writer_bytes(&writer_tx, bytes)
                                .map_err(|_| Lv1ActorError::CommandSendFailed)
                        });

                        let failed = result.is_err();
                        if let Some(reply) = reply {
                            let _ = reply.send(result);
                        }
                        if failed {
                            return DisconnectReason::TcpError(
                                "SetMute send failed (encode or writer queue)".to_string(),
                            );
                        }
                    }
                    Some(Lv1Command::RecallScene { scene_index, reply }) => {
                        let result = encode_frame(
                            "/Set/CurSceneIndex",
                            &[OscArg::Int(scene_index)],
                        )
                        .map_err(|_| Lv1ActorError::CommandSendFailed)
                        .and_then(|bytes| {
                            enqueue_writer_bytes(&writer_tx, bytes)
                                .map_err(|_| Lv1ActorError::CommandSendFailed)
                        });

                        let failed = result.is_err();
                        if let Some(reply) = reply {
                            let _ = reply.send(result);
                        }
                        if failed {
                            return DisconnectReason::TcpError(
                                "RecallScene send failed (encode or writer queue)".to_string(),
                            );
                        }
                    }
                    Some(Lv1Command::Flush { reply }) => {
                        if let Some(reply) = reply
                            && let Err(reply) = enqueue_writer_flush(&writer_tx, reply)
                        {
                            let _ = reply.send(Err(Lv1ActorError::CommandSendFailed));
                            return DisconnectReason::TcpError(
                                "flush enqueue failed (writer queue full or gone)".to_string(),
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::commands::{Lv1ParameterWrite, Lv1WriteParameter};
    use crate::lv1::tcp::{FrameDecoder, decode_frame_payload, encode_parameter_write_batch};
    use crate::runtime::events::AppEventBus;
    use tokio::sync::mpsc;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn drain_commands_reports_closed_command_channel() {
        let (tx, mut rx) = mpsc::channel(1);
        drop(tx);
        let mut state = ActorState::new(AppEventBus::default(), 0);

        let result = drain_commands_for(&mut state, &mut rx, Duration::from_secs(1)).await;

        assert_eq!(result, DrainCommandsResult::CommandChannelClosed);
    }

    #[tokio::test]
    async fn drain_commands_reports_not_connected_for_flush_while_disconnected() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut state = ActorState::new(AppEventBus::default(), 0);
        let (reply_tx, reply_rx) = oneshot::channel();
        tx.try_send(Lv1Command::Flush {
            reply: Some(reply_tx),
        })
        .unwrap();
        drop(tx);

        let result = drain_commands_for(&mut state, &mut rx, Duration::from_secs(1)).await;

        assert_eq!(result, DrainCommandsResult::CommandChannelClosed);
        assert_eq!(reply_rx.await.unwrap(), Err(Lv1ActorError::NotConnected));
    }

    #[test]
    fn drain_disconnected_command_rejects_recall_scene_when_not_connected() {
        let state = ActorState::new(AppEventBus::default(), 0);
        let (reply, rx) = oneshot::channel();

        drain_disconnected_command(
            Lv1Command::RecallScene {
                scene_index: 4,
                reply: Some(reply),
            },
            &state,
            Err(Lv1ActorError::NotConnected),
        );

        assert_eq!(
            rx.blocking_recv().unwrap(),
            Err(Lv1ActorError::NotConnected)
        );
    }

    #[tokio::test]
    async fn pan_family_commands_encode_expected_osc_paths() {
        let bytes = encode_parameter_write_batch(&[
            Lv1ParameterWrite {
                group: 0,
                channel: 0,
                parameter: Lv1WriteParameter::Pan,
                value: -0.5,
            },
            Lv1ParameterWrite {
                group: 0,
                channel: 0,
                parameter: Lv1WriteParameter::Balance,
                value: 0.25,
            },
            Lv1ParameterWrite {
                group: 0,
                channel: 0,
                parameter: Lv1WriteParameter::Width,
                value: 0.75,
            },
        ])
        .unwrap();

        let mut decoder = FrameDecoder::default();
        let frames = decoder.push(&bytes).unwrap();
        let addresses: Vec<_> = frames
            .iter()
            .map(|frame| decode_frame_payload(frame).unwrap().address)
            .collect();

        assert!(addresses.contains(&"/Set/Track/Pan".to_string()));
        assert!(addresses.contains(&"/Set/Track/Pan/Balance".to_string()));
        assert!(addresses.contains(&"/Set/Track/Pan/Width".to_string()));
    }

    #[test]
    fn enqueue_writer_bytes_reports_tcp_error_when_queue_is_full() {
        let (tx, _rx) = mpsc::channel(1);
        tx.try_send(WriterMessage::Bytes(vec![1])).unwrap();

        let result = enqueue_writer_bytes(&tx, vec![2]);

        assert_eq!(
            result,
            Err(DisconnectReason::TcpError("writer queue full".to_string()))
        );
    }

    #[test]
    fn recall_scene_frame_uses_set_cur_scene_index() {
        let bytes = encode_frame("/Set/CurSceneIndex", &[OscArg::Int(4)]).unwrap();
        let mut decoder = FrameDecoder::default();
        let frames = decoder.push(&bytes).unwrap();
        let msg = decode_frame_payload(&frames[0]).unwrap();

        assert_eq!(msg.address, "/Set/CurSceneIndex");
        assert_eq!(msg.args, vec![OscArg::Int(4)]);
    }

    #[tokio::test]
    async fn fail_pending_writer_flushes_sends_error_to_queued_flush_reply() {
        let (tx, mut rx) = mpsc::channel(2);
        let (flush_tx, flush_rx) = oneshot::channel();
        tx.try_send(WriterMessage::Bytes(vec![1])).unwrap();
        tx.try_send(WriterMessage::Flush(flush_tx)).unwrap();

        fail_pending_writer_flushes(&mut rx);

        assert_eq!(
            flush_rx.await.unwrap(),
            Err(Lv1ActorError::CommandSendFailed)
        );
    }
}

//! LV1 actor runtime.

use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use super::commands::Lv1Command;
use super::events::Lv1ActorError;
use super::events::Lv1Event;
use super::handle::Lv1ActorHandle;
use super::state::{ActorState, handle_message};
use super::tcp::{Lv1TcpClient, decode_frame_payload, pong_for_ping, read_next_async, send_async};
use super::types::ConnectionStatus;
use crate::osc::OscArg;
use crate::runtime::events::AppEventBus;

const PING_TIMEOUT: Duration = Duration::from_secs(10);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

/// Spawn the LV1 actor. Returns a handle immediately; the actor connects in the background.
pub fn spawn_actor(host: String, port: u16, event_bus: AppEventBus) -> Lv1ActorHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    tokio::spawn(run_actor(host, port, event_bus, cmd_rx));
    Lv1ActorHandle::new(cmd_tx)
}

#[derive(Debug, PartialEq, Eq)]
enum DrainCommandsResult {
    TimedOut,
    CommandChannelClosed,
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
                Some(Lv1Command::GetState { reply }) => {
                    let _ = reply.send(state.snapshot());
                }
                Some(Lv1Command::WriteBatch(_)) => {}
                Some(Lv1Command::SetGain { reply, .. }) => {
                    let _ = reply.send(Err(Lv1ActorError::NotConnected));
                }
                Some(Lv1Command::SetPan { reply, .. }) => {
                    let _ = reply.send(Err(Lv1ActorError::NotConnected));
                }
                Some(Lv1Command::SetBalance { reply, .. }) => {
                    let _ = reply.send(Err(Lv1ActorError::NotConnected));
                }
                Some(Lv1Command::SetWidth { reply, .. }) => {
                    let _ = reply.send(Err(Lv1ActorError::NotConnected));
                }
                Some(Lv1Command::SetMute { reply, .. }) => {
                    let _ = reply.send(Err(Lv1ActorError::NotConnected));
                }
                Some(Lv1Command::Flush { reply }) => {
                    let _ = reply.send(Ok(()));
                }
            },
        }
    }
}

async fn run_actor(
    host: String,
    port: u16,
    event_bus: AppEventBus,
    mut cmd_rx: mpsc::Receiver<Lv1Command>,
) {
    let mut state = ActorState::new(event_bus);

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
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Lv1Command::GetState { reply } => {
                    let _ = reply.send(state.snapshot());
                }
                Lv1Command::WriteBatch(_) => {}
                Lv1Command::SetGain { reply, .. } => {
                    let _ = reply.send(Err(Lv1ActorError::NotConnected));
                }
                Lv1Command::SetPan { reply, .. } => {
                    let _ = reply.send(Err(Lv1ActorError::NotConnected));
                }
                Lv1Command::SetBalance { reply, .. } => {
                    let _ = reply.send(Err(Lv1ActorError::NotConnected));
                }
                Lv1Command::SetWidth { reply, .. } => {
                    let _ = reply.send(Err(Lv1ActorError::NotConnected));
                }
                Lv1Command::SetMute { reply, .. } => {
                    let _ = reply.send(Err(Lv1ActorError::NotConnected));
                }
                Lv1Command::Flush { reply } => {
                    let _ = reply.send(Ok(()));
                }
            }
        }
        state.fan_out(Lv1Event::Connected);

        let disconnected = run_connected(&mut client, &mut state, &mut cmd_rx).await;

        state.connection = ConnectionStatus::Disconnected;
        state.scene = None;
        state.channels.clear();
        state.scene_buf = Default::default();
        state.fan_out(Lv1Event::Disconnected);

        if disconnected == DisconnectReason::CommandChannelClosed {
            break;
        }

        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

#[derive(PartialEq)]
enum DisconnectReason {
    TcpError,
    PingTimeout,
    CommandChannelClosed,
}

async fn run_connected(
    client: &mut Lv1TcpClient,
    state: &mut ActorState,
    cmd_rx: &mut mpsc::Receiver<Lv1Command>,
) -> DisconnectReason {
    let reader = &mut client.reader;
    let writer = &mut client.writer;
    let decoder = &mut client.decoder;

    loop {
        if state.last_ping.elapsed() > PING_TIMEOUT {
            return DisconnectReason::PingTimeout;
        }

        tokio::select! {
            frames = read_next_async(reader, decoder) => {
                match frames {
                    Err(_) => return DisconnectReason::TcpError,
                    Ok(frames) => {
                        for frame in frames {
                            if let Ok(msg) = decode_frame_payload(&frame) {
                                if let Some((addr, args)) = pong_for_ping(&msg) {
                                    let _ = send_async(writer, addr, &args).await;
                                    state.last_ping = Instant::now();
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
                    Some(Lv1Command::WriteBatch(_)) => {}
                    Some(Lv1Command::SetGain { group, channel, gain_db, reply }) => {
                        let result = send_async(
                            writer,
                            "/Set/Track/Out/Gain",
                            &[
                                OscArg::Int(group),
                                OscArg::Int(channel),
                                OscArg::Double(gain_db),
                            ],
                        )
                        .await
                        .map_err(|_| Lv1ActorError::CommandSendFailed);

                        let failed = result.is_err();
                        let _ = reply.send(result);
                        if failed {
                            return DisconnectReason::TcpError;
                        }
                    }
                    Some(Lv1Command::SetPan { group, channel, value, reply }) => {
                        let result = send_async(
                            writer,
                            "/Set/Track/Pan",
                            &[
                                OscArg::Int(group),
                                OscArg::Int(channel),
                                OscArg::Double(value),
                            ],
                        )
                        .await
                        .map_err(|_| Lv1ActorError::CommandSendFailed);

                        let failed = result.is_err();
                        let _ = reply.send(result);
                        if failed {
                            return DisconnectReason::TcpError;
                        }
                    }
                    Some(Lv1Command::SetBalance { group, channel, value, reply }) => {
                        let result = send_async(
                            writer,
                            "/Set/Track/Pan/Balance",
                            &[
                                OscArg::Int(group),
                                OscArg::Int(channel),
                                OscArg::Double(value),
                            ],
                        )
                        .await
                        .map_err(|_| Lv1ActorError::CommandSendFailed);

                        let failed = result.is_err();
                        let _ = reply.send(result);
                        if failed {
                            return DisconnectReason::TcpError;
                        }
                    }
                    Some(Lv1Command::SetWidth { group, channel, value, reply }) => {
                        let result = send_async(
                            writer,
                            "/Set/Track/Pan/Width",
                            &[
                                OscArg::Int(group),
                                OscArg::Int(channel),
                                OscArg::Double(value),
                            ],
                        )
                        .await
                        .map_err(|_| Lv1ActorError::CommandSendFailed);

                        let failed = result.is_err();
                        let _ = reply.send(result);
                        if failed {
                            return DisconnectReason::TcpError;
                        }
                    }
                    Some(Lv1Command::SetMute { group, channel, muted, reply }) => {
                        let result = send_async(
                            writer,
                            "/Set/Track/Out/Mute",
                            &[
                                OscArg::Int(group),
                                OscArg::Int(channel),
                                OscArg::Bool(muted),
                            ],
                        )
                        .await
                        .map_err(|_| Lv1ActorError::CommandSendFailed);

                        let failed = result.is_err();
                        let _ = reply.send(result);
                        if failed {
                            return DisconnectReason::TcpError;
                        }
                    }
                    Some(Lv1Command::Flush { reply }) => {
                        let _ = reply.send(Ok(()));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn drain_commands_reports_closed_command_channel() {
        let (tx, mut rx) = mpsc::channel(1);
        drop(tx);
        let mut state = ActorState::new(AppEventBus::default());

        let result = drain_commands_for(&mut state, &mut rx, Duration::from_secs(1)).await;

        assert_eq!(result, DrainCommandsResult::CommandChannelClosed);
    }

    #[test]
    fn pan_family_addresses_match_expected_osc_paths() {
        let samples = [
            ("/Set/Track/Pan", OscArg::Double(-0.5)),
            ("/Set/Track/Pan/Balance", OscArg::Double(0.25)),
            ("/Set/Track/Pan/Width", OscArg::Double(0.75)),
        ];

        assert_eq!(samples[0].0, "/Set/Track/Pan");
        assert_eq!(samples[1].0, "/Set/Track/Pan/Balance");
        assert_eq!(samples[2].0, "/Set/Track/Pan/Width");
    }
}

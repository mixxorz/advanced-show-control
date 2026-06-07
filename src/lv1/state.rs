//! LV1 live state mirror — actor, types, commands, and events.

use tokio::sync::{mpsc, oneshot};

use super::messages::{Lv1ActorError, Lv1Command, Lv1Event};
use super::model::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry, SceneState};
use super::parsers::{parse_channels_batch, parse_scene_list};
use crate::osc::OscArg;

// ---------------------------------------------------------------------------
// Group constants (confirmed from hardware logs)
// ---------------------------------------------------------------------------

pub mod group {
    pub const INPUT: i32 = 0;
    pub const GROUP: i32 = 1;
    pub const AUX: i32 = 2;
    pub const LR: i32 = 3;
    pub const CENTER: i32 = 4;
    pub const MONO: i32 = 5;
    pub const MATRIX: i32 = 6;
    pub const CUE: i32 = 7;
    pub const TALKBACK: i32 = 8;
}

/// Pairs `/Notify/CurSceneIndex` and `/Notify/Scene/Name` OSC messages into a
/// complete `SceneState`. LV1 sends these as two separate messages that always
/// arrive close together but in either order. Call `apply_index` and `apply_name`
/// as messages arrive; the buffer emits `Some(SceneState)` once both have been
/// received, then clears itself.
#[derive(Default)]
pub struct SceneBuffer {
    pending_index: Option<i32>,
    pending_name: Option<String>,
}

impl SceneBuffer {
    pub fn apply_index(&mut self, index: i32) -> Option<SceneState> {
        self.pending_index = Some(index);
        self.try_emit()
    }

    pub fn apply_name(&mut self, name: String) -> Option<SceneState> {
        self.pending_name = Some(name);
        self.try_emit()
    }

    fn try_emit(&mut self) -> Option<SceneState> {
        if self.pending_index.is_some() && self.pending_name.is_some() {
            let index = self.pending_index.take().unwrap();
            let name = self.pending_name.take().unwrap();
            Some(SceneState { index, name })
        } else {
            None
        }
    }
}

pub fn apply_fader_update(channels: &mut Vec<ChannelInfo>, group: i32, channel: i32, gain_db: f64) {
    if let Some(ch) = channels
        .iter_mut()
        .find(|c| c.group == group && c.channel == channel)
    {
        ch.gain_db = gain_db;
    }
}

pub fn apply_mute_update(channels: &mut Vec<ChannelInfo>, group: i32, channel: i32, muted: bool) {
    if let Some(ch) = channels
        .iter_mut()
        .find(|c| c.group == group && c.channel == channel)
    {
        ch.muted = muted;
    }
}

pub fn osc_arg_to_bool(arg: &OscArg) -> Option<bool> {
    match arg {
        OscArg::Bool(value) => Some(*value),
        OscArg::True => Some(true),
        OscArg::False => Some(false),
        OscArg::Int(0) => Some(false),
        OscArg::Int(1) => Some(true),
        _ => None,
    }
}

use crate::lv1::tcp::{
    Lv1TcpClient, decode_frame_payload, pong_for_ping, read_next_async, send_async,
};
use std::time::{Duration, Instant};

const PING_TIMEOUT: Duration = Duration::from_secs(10);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

/// A cloneable handle to the LV1 actor. Use this to send commands.
#[derive(Clone)]
pub struct Lv1ActorHandle {
    tx: mpsc::Sender<Lv1Command>,
}

impl Lv1ActorHandle {
    /// Get a point-in-time snapshot of the current state.
    pub async fn get_state(&self) -> Lv1StateSnapshot {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.tx.send(Lv1Command::GetState { reply: reply_tx }).await;
        reply_rx
            .await
            .expect("actor dropped before responding to GetState")
    }

    /// Subscribe to all future events. Returns a receiver for `Lv1Event`.
    pub async fn subscribe(&self) -> mpsc::Receiver<Lv1Event> {
        let (event_tx, event_rx) = mpsc::channel(64);
        let _ = self.tx.send(Lv1Command::Subscribe { tx: event_tx }).await;
        event_rx
    }

    /// Send a `/Set/Track/Out/Gain` command to LV1. Fire and forget.
    pub async fn set_gain(
        &self,
        group: i32,
        channel: i32,
        gain_db: f64,
    ) -> Result<(), Lv1ActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .tx
            .send(Lv1Command::SetGain {
                group,
                channel,
                gain_db,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Lv1ActorError::CommandChannelClosed)?;

        reply_rx
            .await
            .map_err(|_| Lv1ActorError::ReplyChannelClosed)?
    }

    /// Send a `/Set/Track/Out/Mute` command to LV1. Fire and forget.
    pub async fn set_mute(
        &self,
        group: i32,
        channel: i32,
        muted: bool,
    ) -> Result<(), Lv1ActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .tx
            .send(Lv1Command::SetMute {
                group,
                channel,
                muted,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Lv1ActorError::CommandChannelClosed)?;

        reply_rx
            .await
            .map_err(|_| Lv1ActorError::ReplyChannelClosed)?
    }

    /// Wait until all previously queued commands have been processed.
    pub async fn flush(&self) -> Result<(), Lv1ActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Lv1Command::Flush { reply: reply_tx })
            .await
            .map_err(|_| Lv1ActorError::CommandChannelClosed)?;

        reply_rx
            .await
            .map_err(|_| Lv1ActorError::ReplyChannelClosed)?
    }
}

struct ActorState {
    connection: ConnectionStatus,
    scene: Option<SceneState>,
    scene_list: Vec<SceneListEntry>,
    channels: Vec<ChannelInfo>,
    scene_buf: SceneBuffer,
    last_ping: Instant,
    subscribers: Vec<mpsc::Sender<Lv1Event>>,
}

impl ActorState {
    fn new() -> Self {
        Self {
            connection: ConnectionStatus::Connecting,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
            scene_buf: SceneBuffer::default(),
            last_ping: Instant::now(),
            subscribers: Vec::new(),
        }
    }

    fn snapshot(&self) -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: self.connection.clone(),
            scene: self.scene.clone(),
            scene_list: self.scene_list.clone(),
            channels: self.channels.clone(),
        }
    }

    fn fan_out(&mut self, event: Lv1Event) {
        self.subscribers
            .retain(|tx| tx.try_send(event.clone()).is_ok());
    }
}

/// Spawn the LV1 actor. Returns a handle immediately; the actor connects in the background.
pub fn spawn_actor(host: String, port: u16) -> Lv1ActorHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    tokio::spawn(run_actor(host, port, cmd_rx));
    Lv1ActorHandle { tx: cmd_tx }
}

/// Drain pending commands for `duration`, responding to GetState immediately.
/// Used during reconnect delays so callers are never blocked indefinitely.
async fn drain_commands_for(
    state: &mut ActorState,
    cmd_rx: &mut mpsc::Receiver<Lv1Command>,
    duration: Duration,
) {
    let deadline = tokio::time::sleep(duration);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            cmd = cmd_rx.recv() => match cmd {
                None => break,
                Some(Lv1Command::GetState { reply }) => {
                    let _ = reply.send(state.snapshot());
                }
                Some(Lv1Command::Subscribe { tx }) => {
                    state.subscribers.push(tx);
                }
                Some(Lv1Command::SetGain { reply, .. }) => {
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

async fn run_actor(host: String, port: u16, mut cmd_rx: mpsc::Receiver<Lv1Command>) {
    let mut state = ActorState::new();

    loop {
        // --- Connect (drain commands during reconnect sleeps) ---
        let mut client = loop {
            match Lv1TcpClient::connect(&host, port).await {
                Ok(c) => break c,
                Err(_) => {
                    drain_commands_for(&mut state, &mut cmd_rx, RECONNECT_DELAY).await;
                }
            }
        };

        let device_name = "lv1-state-mirror";
        let uuid = uuid::Uuid::new_v4().to_string();
        if client.register_myfoh(device_name, &uuid).await.is_err() {
            drain_commands_for(&mut state, &mut cmd_rx, RECONNECT_DELAY).await;
            continue;
        }

        state.connection = ConnectionStatus::Connected;
        state.last_ping = Instant::now();

        // Yield to let any pending Subscribe commands arrive before we emit Connected.
        tokio::task::yield_now().await;
        // Drain any commands that arrived during connection setup.
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Lv1Command::GetState { reply } => {
                    let _ = reply.send(state.snapshot());
                }
                Lv1Command::Subscribe { tx } => {
                    state.subscribers.push(tx);
                }
                Lv1Command::SetGain { reply, .. } => {
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

        // --- Run loop ---
        let disconnected = run_connected(&mut client, &mut state, &mut cmd_rx).await;

        // --- Disconnect ---
        state.connection = ConnectionStatus::Disconnected;
        state.scene = None;
        state.channels.clear();
        state.scene_buf = SceneBuffer::default();
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
        // Check ping watchdog
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
                                // Handle ping/pong
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

            // Handle commands
            cmd = cmd_rx.recv() => {
                match cmd {
                    None => return DisconnectReason::CommandChannelClosed,
                    Some(Lv1Command::GetState { reply }) => {
                        let _ = reply.send(state.snapshot());
                    }
                    Some(Lv1Command::Subscribe { tx }) => {
                        state.subscribers.push(tx);
                    }
                    Some(Lv1Command::SetGain { group, channel, gain_db, reply }) => {
                        let result = send_async(
                            writer,
                            "/Set/Track/Out/Gain",
                            &[
                                crate::osc::OscArg::Int(group),
                                crate::osc::OscArg::Int(channel),
                                crate::osc::OscArg::Double(gain_db),
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
                                crate::osc::OscArg::Int(group),
                                crate::osc::OscArg::Int(channel),
                                crate::osc::OscArg::Bool(muted),
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

fn handle_message(state: &mut ActorState, msg: &crate::osc::OscMessage) {
    match msg.address.as_str() {
        "/Channels" => {
            if let Ok(channels) = parse_channels_batch(&msg.args) {
                state.channels = channels.clone();
                state.fan_out(Lv1Event::ChannelTopologyChanged(channels));
            }
        }
        "/Notify/CurSceneIndex" => {
            if let Some(crate::osc::OscArg::Int(index)) = msg.args.first() {
                if let Some(scene) = state.scene_buf.apply_index(*index) {
                    state.scene = Some(scene.clone());
                    state.fan_out(Lv1Event::SceneChanged(scene));
                }
            }
        }
        "/Notify/Scene/Name" => {
            if let Some(crate::osc::OscArg::String(name)) = msg.args.first() {
                if let Some(scene) = state.scene_buf.apply_name(name.clone()) {
                    state.scene = Some(scene.clone());
                    state.fan_out(Lv1Event::SceneChanged(scene));
                }
            }
        }
        "/Notify/SceneList" => {
            if let Ok(list) = parse_scene_list(&msg.args) {
                state.scene_list = list.clone();
                state.fan_out(Lv1Event::SceneListChanged(list));
            }
        }
        "/Notify/Track/Out/Gain" => {
            if let (
                Some(crate::osc::OscArg::Int(group)),
                Some(crate::osc::OscArg::Int(channel)),
                Some(crate::osc::OscArg::Double(gain_db)),
            ) = (msg.args.first(), msg.args.get(1), msg.args.get(2))
            {
                apply_fader_update(&mut state.channels, *group, *channel, *gain_db);
                state.fan_out(Lv1Event::FaderChanged {
                    group: *group,
                    channel: *channel,
                    gain_db: *gain_db,
                });
            }
        }
        "/Notify/Track/Out/Mute" => {
            if let (
                Some(crate::osc::OscArg::Int(group)),
                Some(crate::osc::OscArg::Int(channel)),
                Some(mute_arg),
            ) = (msg.args.first(), msg.args.get(1), msg.args.get(2))
            {
                if let Some(muted) = osc_arg_to_bool(mute_arg) {
                    apply_mute_update(&mut state.channels, *group, *channel, muted);
                    state.fan_out(Lv1Event::MuteChanged {
                        group: *group,
                        channel: *channel,
                        muted,
                    });
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_buffer_emits_when_name_arrives_first() {
        let mut buf = SceneBuffer::default();
        assert!(buf.apply_name("Scene A".to_string()).is_none());
        let scene = buf.apply_index(0).unwrap();
        assert_eq!(
            scene,
            SceneState {
                index: 0,
                name: "Scene A".to_string()
            }
        );
        assert!(buf.apply_index(0).is_none());
    }

    #[test]
    fn scene_buffer_emits_when_index_arrives_first() {
        let mut buf = SceneBuffer::default();
        assert!(buf.apply_index(1).is_none());
        let scene = buf.apply_name("Scene B".to_string()).unwrap();
        assert_eq!(
            scene,
            SceneState {
                index: 1,
                name: "Scene B".to_string()
            }
        );
    }

    #[test]
    fn scene_buffer_overwrites_pending_with_new_name() {
        let mut buf = SceneBuffer::default();
        buf.apply_name("Old".to_string());
        buf.apply_name("New".to_string());
        let scene = buf.apply_index(2).unwrap();
        assert_eq!(scene.name, "New");
    }

    #[test]
    fn apply_fader_update_changes_matching_channel() {
        let mut channels = vec![
            ChannelInfo {
                group: 0,
                channel: 0,
                name: "Ch 1".to_string(),
                gain_db: -9.0,
                muted: false,
            },
            ChannelInfo {
                group: 0,
                channel: 1,
                name: "Ch 2".to_string(),
                gain_db: -12.0,
                muted: false,
            },
        ];
        apply_fader_update(&mut channels, 0, 0, -6.0);
        assert_eq!(channels[0].gain_db, -6.0);
        assert_eq!(channels[1].gain_db, -12.0);
    }

    #[test]
    fn apply_fader_update_ignores_unknown_channel() {
        let mut channels = vec![ChannelInfo {
            group: 0,
            channel: 0,
            name: "Ch 1".to_string(),
            gain_db: -9.0,
            muted: false,
        }];
        apply_fader_update(&mut channels, 0, 99, -3.0);
        assert_eq!(channels[0].gain_db, -9.0);
    }

    #[test]
    fn apply_mute_update_changes_matching_channel() {
        let mut channels = vec![
            ChannelInfo {
                group: 0,
                channel: 0,
                name: "Ch 1".to_string(),
                gain_db: -9.0,
                muted: false,
            },
            ChannelInfo {
                group: 0,
                channel: 1,
                name: "Ch 2".to_string(),
                gain_db: -12.0,
                muted: false,
            },
        ];
        apply_mute_update(&mut channels, 0, 0, true);
        assert!(channels[0].muted);
        assert!(!channels[1].muted);
    }

    #[test]
    fn apply_mute_update_ignores_unknown_channel() {
        let mut channels = vec![ChannelInfo {
            group: 0,
            channel: 0,
            name: "Ch 1".to_string(),
            gain_db: -9.0,
            muted: false,
        }];
        apply_mute_update(&mut channels, 0, 99, true);
        assert!(!channels[0].muted);
    }

    #[test]
    fn osc_bool_values_map_to_mute_state() {
        assert_eq!(osc_arg_to_bool(&OscArg::Bool(true)), Some(true));
        assert_eq!(osc_arg_to_bool(&OscArg::Bool(false)), Some(false));
        assert_eq!(osc_arg_to_bool(&OscArg::Int(1)), Some(true));
        assert_eq!(osc_arg_to_bool(&OscArg::Int(0)), Some(false));
        assert_eq!(osc_arg_to_bool(&OscArg::Int(2)), None);
    }
}

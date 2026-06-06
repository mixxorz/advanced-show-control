//! LV1 live state mirror — actor, types, commands, and events.

use tokio::sync::{mpsc, oneshot};

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

// ---------------------------------------------------------------------------
// State types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Disconnected,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneState {
    pub index: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneListEntry {
    pub index: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChannelInfo {
    pub group: i32,
    pub channel: i32,
    pub name: String,
    pub gain_db: f64,
}

#[derive(Debug, Clone)]
pub struct Lv1StateSnapshot {
    pub connection: ConnectionStatus,
    pub scene: Option<SceneState>,
    pub scene_list: Vec<SceneListEntry>,
    pub channels: Vec<ChannelInfo>,
}

// ---------------------------------------------------------------------------
// Commands and events
// ---------------------------------------------------------------------------

pub enum Lv1Command {
    GetState {
        reply: oneshot::Sender<Lv1StateSnapshot>,
    },
    Subscribe {
        tx: mpsc::Sender<Lv1Event>,
    },
    SetGain {
        group: i32,
        channel: i32,
        gain_db: f64,
    },
}

#[derive(Debug, Clone)]
pub enum Lv1Event {
    Connected,
    Disconnected,
    SceneChanged(SceneState),
    SceneListChanged(Vec<SceneListEntry>),
    FaderChanged {
        group: i32,
        channel: i32,
        gain_db: f64,
    },
    ChannelTopologyChanged(Vec<ChannelInfo>),
}

// ---------------------------------------------------------------------------
// Parsers and helpers
// ---------------------------------------------------------------------------

// Each channel record in the /Channels batch has 19 fields:
// [0] s:name, [1] i:group, [2] i:channel, [3] d:gain_db,
// [4..18] other fields (phantom sends, colors, flags — not used in Phase 2)
const CHANNELS_RECORD_STRIDE: usize = 19;

pub fn parse_channels_batch(args: &[OscArg]) -> Result<Vec<ChannelInfo>, &'static str> {
    let count = match args.first() {
        Some(OscArg::Int(n)) => *n as usize,
        _ => return Err("missing or wrong-type count arg"),
    };

    let expected_len = 1 + count * CHANNELS_RECORD_STRIDE;
    if args.len() < expected_len {
        return Err("args too short for declared channel count");
    }

    let mut channels = Vec::with_capacity(count);
    for i in 0..count {
        let base = 1 + i * CHANNELS_RECORD_STRIDE;
        let name = match &args[base] {
            OscArg::String(s) => s.clone(),
            _ => return Err("channel name must be a string"),
        };
        let group = match args[base + 1] {
            OscArg::Int(v) => v,
            _ => return Err("channel group must be an int"),
        };
        let channel = match args[base + 2] {
            OscArg::Int(v) => v,
            _ => return Err("channel index must be an int"),
        };
        let gain_db = match args[base + 3] {
            OscArg::Double(v) => v,
            _ => return Err("channel gain must be a double"),
        };
        channels.push(ChannelInfo { group, channel, name, gain_db });
    }

    Ok(channels)
}

pub fn parse_scene_list(args: &[OscArg]) -> Result<Vec<SceneListEntry>, &'static str> {
    let count = match args.first() {
        Some(OscArg::Int(n)) => *n as usize,
        _ => return Err("missing or wrong-type count arg"),
    };

    let expected_len = 1 + count * 2;
    if args.len() < expected_len {
        return Err("args too short for declared scene count");
    }

    let mut list = Vec::with_capacity(count);
    for i in 0..count {
        let base = 1 + i * 2;
        let index = match args[base] {
            OscArg::Int(v) => v,
            _ => return Err("scene index must be an int"),
        };
        let name = match &args[base + 1] {
            OscArg::String(s) => s.clone(),
            _ => return Err("scene name must be a string"),
        };
        list.push(SceneListEntry { index, name });
    }

    Ok(list)
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
    if let Some(ch) = channels.iter_mut().find(|c| c.group == group && c.channel == channel) {
        ch.gain_db = gain_db;
    }
}

use crate::lv1::tcp::{Lv1TcpClient, decode_frame_payload, pong_for_ping, read_next_async, send_async};
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
        reply_rx.await.expect("actor dropped before responding to GetState")
    }

    /// Subscribe to all future events. Returns a receiver for `Lv1Event`.
    pub async fn subscribe(&self) -> mpsc::Receiver<Lv1Event> {
        let (event_tx, event_rx) = mpsc::channel(64);
        let _ = self.tx.send(Lv1Command::Subscribe { tx: event_tx }).await;
        event_rx
    }

    /// Send a `/Set/Track/Out/Gain` command to LV1. Fire and forget.
    pub async fn set_gain(&self, group: i32, channel: i32, gain_db: f64) {
        let _ = self.tx.send(Lv1Command::SetGain { group, channel, gain_db }).await;
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
        self.subscribers.retain(|tx| tx.try_send(event.clone()).is_ok());
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
                Some(Lv1Command::SetGain { .. }) => {
                    // Silently drop — not connected
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
                Lv1Command::GetState { reply } => { let _ = reply.send(state.snapshot()); }
                Lv1Command::Subscribe { tx } => { state.subscribers.push(tx); }
                Lv1Command::SetGain { .. } => {
                    // Silently drop — not connected yet
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
                    Some(Lv1Command::SetGain { group, channel, gain_db }) => {
                        let _ = send_async(
                            writer,
                            "/Set/Track/Out/Gain",
                            &[
                                crate::osc::OscArg::Int(group),
                                crate::osc::OscArg::Int(channel),
                                crate::osc::OscArg::Double(gain_db),
                            ],
                        ).await;
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
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_channel_args(channels: &[(&str, i32, i32, f64)]) -> Vec<OscArg> {
        let mut args = vec![OscArg::Int(channels.len() as i32)];
        for (name, group, channel, gain_db) in channels {
            args.push(OscArg::String(name.to_string()));
            args.push(OscArg::Int(*group));
            args.push(OscArg::Int(*channel));
            args.push(OscArg::Double(*gain_db));
            for _ in 0..15 {
                args.push(OscArg::Int(0));
            }
        }
        args
    }

    #[test]
    fn parses_channels_batch() {
        let args = make_channel_args(&[
            ("Channel 1", 0, 0, -9.1),
            ("Fx 1", 2, 0, -12.0),
        ]);
        let channels = parse_channels_batch(&args).unwrap();
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0], ChannelInfo { group: 0, channel: 0, name: "Channel 1".to_string(), gain_db: -9.1 });
        assert_eq!(channels[1], ChannelInfo { group: 2, channel: 0, name: "Fx 1".to_string(), gain_db: -12.0 });
    }

    #[test]
    fn rejects_channels_batch_with_wrong_arg_count() {
        let args = vec![OscArg::Int(1)];
        assert!(parse_channels_batch(&args).is_err());
    }

    #[test]
    fn rejects_channels_batch_missing_count() {
        assert!(parse_channels_batch(&[]).is_err());
    }

    #[test]
    fn parses_scene_list_with_multiple_scenes() {
        let args = vec![
            OscArg::Int(2),
            OscArg::Int(0),
            OscArg::String("My first scene".to_string()),
            OscArg::Int(1),
            OscArg::String("My second scene".to_string()),
        ];
        let list = parse_scene_list(&args).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], SceneListEntry { index: 0, name: "My first scene".to_string() });
        assert_eq!(list[1], SceneListEntry { index: 1, name: "My second scene".to_string() });
    }

    #[test]
    fn parses_empty_scene_list() {
        let args = vec![OscArg::Int(0)];
        let list = parse_scene_list(&args).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn rejects_scene_list_missing_count() {
        assert!(parse_scene_list(&[]).is_err());
    }

    #[test]
    fn scene_buffer_emits_when_name_arrives_first() {
        let mut buf = SceneBuffer::default();
        assert!(buf.apply_name("Scene A".to_string()).is_none());
        let scene = buf.apply_index(0).unwrap();
        assert_eq!(scene, SceneState { index: 0, name: "Scene A".to_string() });
        assert!(buf.apply_index(0).is_none());
    }

    #[test]
    fn scene_buffer_emits_when_index_arrives_first() {
        let mut buf = SceneBuffer::default();
        assert!(buf.apply_index(1).is_none());
        let scene = buf.apply_name("Scene B".to_string()).unwrap();
        assert_eq!(scene, SceneState { index: 1, name: "Scene B".to_string() });
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
            ChannelInfo { group: 0, channel: 0, name: "Ch 1".to_string(), gain_db: -9.0 },
            ChannelInfo { group: 0, channel: 1, name: "Ch 2".to_string(), gain_db: -12.0 },
        ];
        apply_fader_update(&mut channels, 0, 0, -6.0);
        assert_eq!(channels[0].gain_db, -6.0);
        assert_eq!(channels[1].gain_db, -12.0);
    }

    #[test]
    fn apply_fader_update_ignores_unknown_channel() {
        let mut channels = vec![
            ChannelInfo { group: 0, channel: 0, name: "Ch 1".to_string(), gain_db: -9.0 },
        ];
        apply_fader_update(&mut channels, 0, 99, -3.0);
        assert_eq!(channels[0].gain_db, -9.0);
    }

    use crate::lv1::tcp::encode_frame;
    use std::net::TcpListener;
    use std::io::Write;

    fn make_lv1_frame(address: &str, args: &[OscArg]) -> Vec<u8> {
        encode_frame(address, args).unwrap()
    }

    #[tokio::test]
    async fn actor_connects_and_emits_connected_event() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::task::spawn_blocking(move || {
            let (_stream, _) = listener.accept().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(200));
        });

        let handle = spawn_actor("127.0.0.1".to_string(), port);
        let mut events = handle.subscribe().await;

        let event = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            events.recv(),
        ).await.unwrap().unwrap();

        assert!(matches!(event, Lv1Event::Connected));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn actor_emits_disconnected_and_reconnects_when_server_closes() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        let _server = tokio::task::spawn_blocking(move || {
            for i in 0..2 {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if i == 0 {
                            // First connection: immediately drop
                            drop(stream);
                        } else {
                            // Second connection: keep alive
                            std::thread::sleep(std::time::Duration::from_secs(2));
                        }
                    }
                    Err(e) => {
                        eprintln!("Accept error: {}", e);
                        break;
                    }
                }
            }
        });

        let handle = spawn_actor("127.0.0.1".to_string(), port);
        let mut events = handle.subscribe().await;

        let mut got_disconnect = false;
        let mut got_reconnect = false;
        let deadline = std::time::Duration::from_secs(10);
        let result = tokio::time::timeout(deadline, async {
            while let Some(event) = events.recv().await {
                match event {
                    Lv1Event::Disconnected => got_disconnect = true,
                    Lv1Event::Connected if got_disconnect => {
                        got_reconnect = true;
                        break;
                    }
                    _ => {}
                }
            }
        }).await;
        assert!(result.is_ok(), "timed out waiting for reconnect");
        assert!(got_reconnect);
    }

    #[tokio::test]
    async fn actor_parses_and_emits_scene_changed() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
            stream.write_all(&make_lv1_frame("/Notify/Scene/Name", &[OscArg::String("Scene A".to_string())])).unwrap();
            stream.write_all(&make_lv1_frame("/Notify/CurSceneIndex", &[OscArg::Int(0)])).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(200));
        });

        let handle = spawn_actor("127.0.0.1".to_string(), port);
        let mut events = handle.subscribe().await;

        let mut scene_event = None;
        let deadline = std::time::Duration::from_secs(3);
        tokio::time::timeout(deadline, async {
            while let Some(event) = events.recv().await {
                if let Lv1Event::SceneChanged(s) = event {
                    scene_event = Some(s);
                    break;
                }
            }
        }).await.unwrap();

        let scene = scene_event.unwrap();
        assert_eq!(scene.index, 0);
        assert_eq!(scene.name, "Scene A");
    }

    #[tokio::test]
    async fn get_state_returns_snapshot_with_current_values() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(500));
        });

        let handle = spawn_actor("127.0.0.1".to_string(), port);
        let mut events = handle.subscribe().await;

        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while let Some(event) = events.recv().await {
                if matches!(event, Lv1Event::Connected) { break; }
            }
        }).await.unwrap();

        let snapshot = handle.get_state().await;
        assert_eq!(snapshot.connection, ConnectionStatus::Connected);
    }

    #[tokio::test]
    async fn actor_handles_set_gain_command() {
        use std::io::Read;

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));

            // Read what the actor sends after SetGain
            let mut buf = [0u8; 4096];
            stream.set_read_timeout(Some(std::time::Duration::from_millis(500))).unwrap();
            let _ = stream.read(&mut buf); // drain handshake bytes sent by actor

            // Keep alive briefly
            std::thread::sleep(std::time::Duration::from_millis(500));
        });

        let handle = spawn_actor("127.0.0.1".to_string(), port);
        let mut events = handle.subscribe().await;

        // Wait for connected
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while let Some(e) = events.recv().await {
                if matches!(e, Lv1Event::Connected) { break; }
            }
        }).await.unwrap();

        // Should not panic — SetGain command is accepted
        handle.set_gain(0, 0, -20.0).await;
    }

    #[tokio::test]
    async fn actor_sends_set_gain_while_waiting_for_input() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (address_tx, address_rx) = std::sync::mpsc::channel();

        tokio::task::spawn_blocking(move || {
            use std::io::Read;

            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(std::time::Duration::from_millis(50))).unwrap();

            let mut buf = [0_u8; 1024];
            let mut decoder = crate::lv1::tcp::FrameDecoder::default();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        for frame in decoder.push(&buf[..n]).unwrap() {
                            let msg = decode_frame_payload(&frame).unwrap();
                            let _ = address_tx.send(msg.address);
                        }
                    }
                    Err(err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            || err.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(err) => panic!("server read failed: {err}"),
                }
            }
        });

        let handle = spawn_actor("127.0.0.1".to_string(), port);
        let mut events = handle.subscribe().await;

        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while let Some(e) = events.recv().await {
                if matches!(e, Lv1Event::Connected) { break; }
            }
        }).await.unwrap();

        let sent_at = std::time::Instant::now();
        handle.set_gain(0, 1, -12.5).await;

        tokio::task::spawn_blocking(move || {
            loop {
                let address = address_rx
                    .recv_timeout(std::time::Duration::from_millis(150))
                    .expect("SetGain frame was not sent promptly while actor was waiting for input");
                if address == "/Set/Track/Out/Gain" {
                    assert!(sent_at.elapsed() < std::time::Duration::from_millis(150));
                    break;
                }
            }
        }).await.unwrap();
    }
}

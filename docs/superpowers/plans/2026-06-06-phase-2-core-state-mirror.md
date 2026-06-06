# Phase 2: Core State Mirror Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `lv1::state` — a tokio actor that maintains a live mirror of LV1 connection status, scene, scene list, and channel/fader values, broadcasting typed events to consumers — plus a `monitor` CLI subcommand.

**Architecture:** A single `Lv1Actor` tokio task owns all state exclusively and processes TCP frames and typed commands via `tokio::select!`. Consumers send `Lv1Command` messages and receive `Lv1Event` notifications through channels. The existing `lv1::tcp`, `lv1::discovery`, and `osc` modules are unchanged.

**Tech Stack:** Rust, tokio (async runtime, `mpsc`/`oneshot` channels, `time::Instant`), existing `lv1::tcp::Lv1TcpClient`.

---

## File Map

| File | Status | Responsibility |
|---|---|---|
| `src/lv1/state.rs` | Create | All state types, group constants, commands, events, message parsers, actor loop |
| `src/lv1/mod.rs` | Modify | Add `pub mod state;` |
| `src/lib.rs` | No change | Already re-exports `lv1` |
| `src/main.rs` | Modify | Add `monitor` subcommand using the actor |
| `Cargo.toml` | Modify | Add `tokio` dependency |

---

## Task 1: Add tokio to Cargo.toml

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add tokio dependency**

Open `Cargo.toml` and add tokio under `[dependencies]`:

```toml
tokio = { version = "1", features = ["full"] }
```

Final `[dependencies]` section:

```toml
[dependencies]
clap = { version = "4.5", features = ["derive"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
socket2 = { version = "0.5", features = ["all"] }
thiserror = "2.0"
tokio = { version = "1", features = ["full"] }
uuid = { version = "1.8", features = ["v4"] }
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add tokio dependency"
```

---

## Task 2: State types and group constants

**Files:**
- Create: `src/lv1/state.rs`
- Modify: `src/lv1/mod.rs`

- [ ] **Step 1: Create `src/lv1/state.rs` with types**

```rust
//! LV1 live state mirror — actor, types, commands, and events.

use tokio::sync::{mpsc, oneshot};

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
```

- [ ] **Step 2: Add `pub mod state;` to `src/lv1/mod.rs`**

```rust
pub mod discovery;
pub mod probe;
pub mod state;
pub mod tcp;
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/lv1/state.rs src/lv1/mod.rs
git commit -m "feat: add lv1 state types and group constants"
```

---

## Task 3: Parse `/Channels` batch

**Files:**
- Modify: `src/lv1/state.rs`

- [ ] **Step 1: Write the failing test**

Add inside `src/lv1/state.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::osc::OscArg;

    fn make_channel_args(channels: &[(&str, i32, i32, f64)]) -> Vec<OscArg> {
        // count + 19 fields per channel; fields [0..3] are name/group/channel/gain,
        // fields [4..18] are padding zeros
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
        // Only the count arg, no channel records
        let args = vec![OscArg::Int(1)];
        assert!(parse_channels_batch(&args).is_err());
    }

    #[test]
    fn rejects_channels_batch_missing_count() {
        assert!(parse_channels_batch(&[]).is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test parse_channels_batch
```

Expected: FAIL — `parse_channels_batch` not defined.

- [ ] **Step 3: Implement `parse_channels_batch`**

Add to `src/lv1/state.rs` (before the `#[cfg(test)]` block):

```rust
use crate::osc::OscArg;

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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test parse_channels_batch
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/lv1/state.rs
git commit -m "feat: add /Channels batch parser"
```

---

## Task 4: Parse `/Notify/SceneList`

**Files:**
- Modify: `src/lv1/state.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src/lv1/state.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test parses_scene_list
```

Expected: FAIL — `parse_scene_list` not defined.

- [ ] **Step 3: Implement `parse_scene_list`**

Add to `src/lv1/state.rs` (before `#[cfg(test)]`):

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test parses_scene_list parses_empty_scene_list rejects_scene_list
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/lv1/state.rs
git commit -m "feat: add /Notify/SceneList parser"
```

---

## Task 5: Scene index/name buffering logic

**Files:**
- Modify: `src/lv1/state.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:

```rust
    #[test]
    fn scene_buffer_emits_when_name_arrives_first() {
        let mut buf = SceneBuffer::default();
        assert!(buf.apply_name("Scene A".to_string()).is_none());
        let scene = buf.apply_index(0).unwrap();
        assert_eq!(scene, SceneState { index: 0, name: "Scene A".to_string() });
        // buffer should be cleared
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test scene_buffer
```

Expected: FAIL — `SceneBuffer` not defined.

- [ ] **Step 3: Implement `SceneBuffer`**

Add to `src/lv1/state.rs` (before `#[cfg(test)]`):

```rust
#[derive(Default)]
pub struct SceneBuffer {
    pending_index: Option<i32>,
    pending_name: Option<String>,
}

impl SceneBuffer {
    /// Call when `/Notify/CurSceneIndex` arrives. Returns `Some(SceneState)` if
    /// the matching name was already buffered.
    pub fn apply_index(&mut self, index: i32) -> Option<SceneState> {
        self.pending_index = Some(index);
        self.try_emit()
    }

    /// Call when `/Notify/Scene/Name` arrives. Returns `Some(SceneState)` if
    /// the matching index was already buffered.
    pub fn apply_name(&mut self, name: String) -> Option<SceneState> {
        self.pending_name = Some(name);
        self.try_emit()
    }

    fn try_emit(&mut self) -> Option<SceneState> {
        if let (Some(index), Some(name)) = (self.pending_index, self.pending_name.take()) {
            self.pending_index = None;
            Some(SceneState { index, name })
        } else {
            None
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test scene_buffer
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/lv1/state.rs
git commit -m "feat: add scene index/name buffering logic"
```

---

## Task 6: Fader update helper

**Files:**
- Modify: `src/lv1/state.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test apply_fader_update
```

Expected: FAIL — `apply_fader_update` not defined.

- [ ] **Step 3: Implement `apply_fader_update`**

Add to `src/lv1/state.rs` (before `#[cfg(test)]`):

```rust
pub fn apply_fader_update(channels: &mut Vec<ChannelInfo>, group: i32, channel: i32, gain_db: f64) {
    if let Some(ch) = channels.iter_mut().find(|c| c.group == group && c.channel == channel) {
        ch.gain_db = gain_db;
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test apply_fader_update
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/lv1/state.rs
git commit -m "feat: add fader update helper"
```

---

## Task 7: Actor handle and spawn

**Files:**
- Modify: `src/lv1/state.rs`

- [ ] **Step 1: Add the actor handle and internal state struct**

Add to `src/lv1/state.rs` (before `#[cfg(test)]`):

```rust
use crate::lv1::tcp::{Lv1TcpClient, decode_frame_payload, pong_for_ping};
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
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/lv1/state.rs
git commit -m "feat: add lv1 actor handle and internal state"
```

---

## Task 8: Actor loop — connected path

**Files:**
- Modify: `src/lv1/state.rs`

- [ ] **Step 1: Add the actor run loop and spawn function**

Add to `src/lv1/state.rs` (before `#[cfg(test)]`):

```rust
/// Spawn the LV1 actor. Returns a handle immediately; the actor connects in the background.
pub fn spawn_actor(host: String, port: u16) -> Lv1ActorHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    tokio::spawn(run_actor(host, port, cmd_rx));
    Lv1ActorHandle { tx: cmd_tx }
}

async fn run_actor(host: String, port: u16, mut cmd_rx: mpsc::Receiver<Lv1Command>) {
    let mut state = ActorState::new();

    loop {
        // --- Connect ---
        let mut client = loop {
            match Lv1TcpClient::connect(&host, port) {
                Ok(c) => break c,
                Err(_) => {
                    tokio::time::sleep(RECONNECT_DELAY).await;
                }
            }
        };

        let device_name = "lv1-state-mirror";
        let uuid = uuid::Uuid::new_v4().to_string();
        if client.register_myfoh(device_name, &uuid).is_err() {
            tokio::time::sleep(RECONNECT_DELAY).await;
            continue;
        }

        state.connection = ConnectionStatus::Connected;
        state.last_ping = Instant::now();
        state.fan_out(Lv1Event::Connected);

        // --- Run loop ---
        let disconnected = run_connected(&mut client, &mut state, &mut cmd_rx).await;

        // --- Disconnect ---
        state.connection = ConnectionStatus::Disconnected;
        state.scene = None;
        state.channels.clear();
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
    loop {
        // Check ping watchdog
        if state.last_ping.elapsed() > PING_TIMEOUT {
            return DisconnectReason::PingTimeout;
        }

        tokio::select! {
            // Poll TCP (non-blocking — read_available uses a 250ms timeout internally)
            _ = tokio::time::sleep(Duration::from_millis(10)) => {
                match client.read_available() {
                    Err(_) => return DisconnectReason::TcpError,
                    Ok(frames) => {
                        for frame in frames {
                            if let Ok(msg) = decode_frame_payload(&frame) {
                                // Handle ping/pong
                                if let Some((addr, args)) = pong_for_ping(&msg) {
                                    let _ = client.send(addr, &args);
                                    state.last_ping = Instant::now();
                                    continue;
                                }
                                handle_message(state, &msg, client);
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
                }
            }
        }
    }
}

fn handle_message(state: &mut ActorState, msg: &crate::osc::OscMessage, _client: &mut Lv1TcpClient) {
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
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/lv1/state.rs
git commit -m "feat: add lv1 actor run loop"
```

---

## Task 9: Actor integration tests

**Files:**
- Modify: `src/lv1/state.rs`

- [ ] **Step 1: Write integration test — actor receives events from fake server**

Add to the `tests` module (note: integration tests that use `tokio::test` need `#[tokio::test]`):

```rust
    use crate::lv1::tcp::{encode_frame, FrameDecoder};
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
            let (mut stream, _) = listener.accept().unwrap();
            // Send handshake ack
            stream.write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
            // Keep alive briefly
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

        tokio::task::spawn_blocking(move || {
            // First connection: accept and immediately drop
            let (_stream, _) = listener.accept().unwrap();
            // Second connection: keep alive
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(500));
        });

        let handle = spawn_actor("127.0.0.1".to_string(), port);
        let mut events = handle.subscribe().await;

        // Should get Connected, then Disconnected, then Connected again
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

        // Wait for Connected
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while let Some(event) = events.recv().await {
                if matches!(event, Lv1Event::Connected) { break; }
            }
        }).await.unwrap();

        let snapshot = handle.get_state().await;
        assert_eq!(snapshot.connection, ConnectionStatus::Connected);
    }
```

- [ ] **Step 2: Run integration tests**

```bash
cargo test --lib actor_connects actor_emits_disconnected actor_parses_and_emits get_state_returns
```

Expected: all pass.

- [ ] **Step 3: Commit**

```bash
git add src/lv1/state.rs
git commit -m "test: add lv1 actor integration tests"
```

---

## Task 10: `monitor` CLI subcommand

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add the `monitor` subcommand variant to the `Command` enum in `src/main.rs`**

Add to the `Command` enum (after `SetGain`):

```rust
    #[command(about = "Connect to an LV1 device and print state changes to the terminal")]
    Monitor {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long, default_value_t = 6000)]
        timeout_ms: u64,
    },
```

- [ ] **Step 2: Add the match arm in `main`**

Add to the `match cli.command` block:

```rust
        Command::Monitor {
            host,
            port,
            timeout_ms,
        } => run_monitor(host, port, timeout_ms),
```

- [ ] **Step 3: Add the `run_monitor` function**

Add to `src/main.rs`:

```rust
fn run_monitor(
    host: Option<String>,
    port: Option<u16>,
    timeout_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    use lv1_scene_fade_utility::lv1::state::{Lv1Event, spawn_actor};

    let (host, port) = resolve_target(host, port, timeout_ms)?;
    eprintln!("connecting to {host}:{port}");

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let handle = spawn_actor(host.clone(), port);
        let mut events = handle.subscribe().await;

        while let Some(event) = events.recv().await {
            match event {
                Lv1Event::Connected => println!("[connected] {host}:{port}"),
                Lv1Event::Disconnected => println!("[disconnected] reconnecting in 3s..."),
                Lv1Event::SceneChanged(scene) => {
                    println!("[scene] index={} name={:?}", scene.index, scene.name);
                }
                Lv1Event::SceneListChanged(list) => {
                    println!("[scene-list] {} scenes", list.len());
                    for entry in &list {
                        println!("  [{}] {:?}", entry.index, entry.name);
                    }
                }
                Lv1Event::FaderChanged { group, channel, gain_db } => {
                    println!("[fader] group={group} ch={channel} {gain_db:.1} dB");
                }
                Lv1Event::ChannelTopologyChanged(channels) => {
                    println!("[channels] {} channels loaded", channels.len());
                }
            }
        }
    });

    Ok(())
}
```

- [ ] **Step 4: Add the `monitor` CLI test**

Add to the `tests` module in `src/main.rs`:

```rust
    #[test]
    fn parses_monitor_command() {
        let cli = Cli::try_parse_from([
            "lv1-probe",
            "monitor",
            "--host",
            "192.168.1.10",
            "--port",
            "50000",
            "--timeout-ms",
            "3000",
        ])
        .unwrap();

        match cli.command {
            Command::Monitor { host, port, timeout_ms } => {
                assert_eq!(host.as_deref(), Some("192.168.1.10"));
                assert_eq!(port, Some(50000));
                assert_eq!(timeout_ms, 3000);
            }
            other => panic!("expected monitor command, got {other:?}"),
        }
    }
```

- [ ] **Step 5: Run tests**

```bash
cargo test parses_monitor_command
```

Expected: pass.

- [ ] **Step 6: Build and smoke-test the binary**

```bash
cargo build
./target/debug/lv1-scene-fade-utility monitor --help
```

Expected: help text showing monitor subcommand with `--host`, `--port`, `--timeout-ms` options.

- [ ] **Step 7: Commit**

```bash
git add src/main.rs
git commit -m "feat: add monitor CLI subcommand"
```

---

## Task 11: Run full test suite

- [ ] **Step 1: Run all tests**

```bash
cargo test
```

Expected: all tests pass, no warnings about unused code in the new module.

- [ ] **Step 2: If any test fails, fix it before continuing**

- [ ] **Step 3: Final commit if any fixes were needed**

```bash
git add -p
git commit -m "fix: address test failures after integration"
```

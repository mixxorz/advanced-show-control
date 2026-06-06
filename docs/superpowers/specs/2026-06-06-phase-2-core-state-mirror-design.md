# Phase 2: Core State Mirror Design

## Purpose

Phase 2 builds the internal live model of LV1 state. It adds `lv1::state` — a Rust actor module that maintains a continuously updated mirror of LV1 connection status, current scene, scene list, and channel/fader values, and broadcasts typed events to consumers.

This is the foundation that the fade engine (Phase 3), UI (Phase 6), and control API (Phase 8) will build on. It must be correct, safe under disconnection, and easy to consume.

## Scope

Included:

- `lv1::state` module with actor, state types, commands, and events.
- Parsing of `/Channels`, `/Notify/CurSceneIndex`, `/Notify/Scene/Name`, `/Notify/SceneList`, and `/Notify/Track/Out/Gain`.
- Connection watchdog with auto-reconnect.
- `monitor` CLI subcommand (human-readable event stream to terminal).
- `tokio` async runtime added to the crate.
- Automated tests for state parsing and actor behavior.

Excluded:

- Fade scheduling or capture logic.
- Project file storage.
- Desktop UI.
- HTTP/WebSocket API.
- Aux send levels (`/Notify/Aux/Send/Pos` etc.) — these are per-channel send amounts, not master faders, and are not needed for fading.

## Confirmed Protocol Behavior

From hardware log analysis across three sessions:

- `/Channels` is pushed by LV1 as part of every scene recall sequence, arriving ~30–300ms **before** `/Notify/CurSceneIndex`. It carries fresh fader values for all channels. No client-initiated request is needed.
- `/Notify/CurSceneIndex` and `/Notify/Scene/Name` always arrive at the same timestamp. Either may arrive first; the actor buffers one and emits `SceneChanged` when both are in hand.
- `/Notify/SceneList` is pushed when scenes are created or deleted: `i:count, [i:index, s:name]*`.
- `/Notify/Track/Out/Gain` fires for physical and on-screen fader moves: `i:group, i:channel, d:gain_db, T/F:from_surface`. The 4th arg `T:true` means the move originated from a user/surface. `F:false` is expected to be the echo of app-sent gain commands (unconfirmed — open question for Phase 3).
- No fader notifications are sent by LV1 during scene recall itself — the fresh `/Channels` batch covers this.

## Architecture

Phase 2 adds one new module. The existing modules are unchanged.

```
src/
  lv1/
    mod.rs
    discovery.rs      (existing)
    tcp.rs            (existing)
    probe.rs          (existing, Phase 1 only)
    state.rs          (new)
  osc.rs              (existing)
  lib.rs              (existing)
  main.rs             (add `monitor` subcommand)
```

`lv1::state` is a self-contained actor. Consumers never touch `Lv1TcpClient` directly — the actor owns it.

## State Types

```rust
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Disconnected,
}

pub struct SceneState {
    pub index: i32,   // -1 = no scene loaded
    pub name: String,
}

pub struct SceneListEntry {
    pub index: i32,
    pub name: String,
}

pub struct ChannelInfo {
    pub group: i32,
    pub channel: i32,
    pub name: String,
    pub gain_db: f64,
}

pub struct Lv1StateSnapshot {
    pub connection: ConnectionStatus,
    pub scene: Option<SceneState>,
    pub scene_list: Vec<SceneListEntry>,
    pub channels: Vec<ChannelInfo>,
}
```

### Group Constants

Confirmed from hardware (`/Channels` batch, 150 channels across 11 group values):

```rust
pub mod group {
    pub const INPUT: i32 = 0;      // CH 1–80 (input channels)
    pub const GROUP: i32 = 1;      // Mix groups
    pub const AUX: i32 = 2;        // Fx 1–8, Mon 1–24 (aux/monitor master faders)
    pub const LR: i32 = 3;
    pub const CENTER: i32 = 4;
    pub const MONO: i32 = 5;
    pub const MATRIX: i32 = 6;
    pub const CUE: i32 = 7;
    pub const TALKBACK: i32 = 8;
}
```

Aux master faders are included in the `/Channels` batch as group 2. No separate handling is required.

## Commands and Events

```rust
pub enum Lv1Command {
    GetState { reply: oneshot::Sender<Lv1StateSnapshot> },
    Subscribe { tx: mpsc::Sender<Lv1Event> },
}

pub enum Lv1Event {
    Connected,
    Disconnected,
    SceneChanged(SceneState),
    SceneListChanged(Vec<SceneListEntry>),
    FaderChanged { group: i32, channel: i32, gain_db: f64 },
    ChannelTopologyChanged(Vec<ChannelInfo>),
}
```

`GetState` returns a snapshot clone immediately, even while disconnected — the `connection` field tells the caller whether values are current. `Subscribe` adds a sender to the actor's internal fan-out list. Dead subscribers (dropped receivers) are pruned silently on the next fan-out.

## Actor Loop

The actor is a tokio task. Its loop does `tokio::select!` between:

1. **TCP frames** from `Lv1TcpClient::read_available()` — parsed and applied to internal state, relevant events fanned out.
2. **Commands** from the command channel — `GetState` or `Subscribe`.

### Message Handling

| Message | Action |
|---|---|
| `/Channels` | Replace `channels` with parsed batch. Fan out `ChannelTopologyChanged`. |
| `/Notify/CurSceneIndex` | Buffer index. If name already buffered, emit `SceneChanged` and clear buffer. |
| `/Notify/Scene/Name` | Buffer name. If index already buffered, emit `SceneChanged` and clear buffer. |
| `/Notify/SceneList` | Replace `scene_list`. Fan out `SceneListChanged`. |
| `/Notify/Track/Out/Gain` | Update matching channel's `gain_db` in `channels`. Fan out `FaderChanged`. |
| `/ping` | Send `/pong` with same args. Record `last_ping_ms`. |

All other messages are silently ignored.

### /Channels Parsing

The batch format is: `i:count`, then records of exactly 19 fields each. Fields within each record:

- `[0]` `s:name`
- `[1]` `i:group`
- `[2]` `i:channel`
- `[3]` `d:gain_db`
- `[4..18]` other fields (ignored for Phase 2)

## Connection Watchdog

Built into the actor loop — no separate task. Two failure conditions:

1. `read_available()` returns EOF or IO error.
2. No `/ping` received within 10 seconds.

On disconnect:

- Set `connection = Disconnected`.
- Fan out `Lv1Event::Disconnected`.
- Clear `scene` and `channels` (stale fader values must not be used for fading).
- Wait 3 seconds.
- Attempt reconnect. On success, send MyFOH handshake and fan out `Lv1Event::Connected`.

`GetState` always responds immediately regardless of connection state.

## Monitor CLI Subcommand

Added to `main.rs` alongside the existing `discover`, `listen`, and `set-gain` subcommands.

```
lv1-probe monitor [--host <ip>] [--port <n>] [--timeout-ms <n>]
```

Subscribes to the actor's event stream and prints human-readable output:

```
[connected] 192.168.1.10:50000
[scene] index=0 name="My first scene"
[fader] group=0 ch=0 "Channel 1" -8.6 dB
[fader] group=0 ch=1 "Channel 2" -2.7 dB
[scene] index=1 name="My second scene"
[disconnected] reconnecting in 3s...
[connected] 192.168.1.10:50000
```

## Error Handling

- Malformed `/Channels` batches (wrong stride, missing fields, wrong arg types) are logged and discarded. The existing channel state is preserved.
- Malformed `/Notify/Track/Out/Gain` messages are discarded.
- Unknown OSC addresses are silently ignored.
- Parse errors never crash the actor.

## Testing

- Unit tests for `/Channels` batch parsing (valid, malformed, partial).
- Unit tests for `/Notify/SceneList` parsing.
- Unit tests for scene index/name buffering logic (both arrival orders).
- Unit tests for fader update application.
- Integration test for actor: spawn actor against a fake TCP server, send known frames, assert events received.
- Integration test for watchdog: fake server closes connection, assert `Disconnected` event fires.

## Open Questions For Phase 3

- Does `/Notify/Track/Out/Gain` with `F:false` on the 4th arg represent the echo of an app-sent `/Set/Track/Out/Gain` command? This is the expected mechanism for manual override detection in Phase 3.
- What is the safe maximum message rate for simultaneous fader gain commands? Not needed for Phase 2 but should be tested before Phase 3.

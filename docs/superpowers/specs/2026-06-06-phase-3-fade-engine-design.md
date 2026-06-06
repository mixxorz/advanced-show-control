# Phase 3: Fade Engine Design

## Purpose

Phase 3 builds the fade engine — a tokio actor that animates selected LV1 faders from their current live positions to stored target values over a configured duration. It is the first component that actively controls LV1 hardware state.

This is a prototype phase. The goal is a working, tested fade engine with a real-hardware CLI smoke test. The UI (Phase 6) and scene watcher (Phase 7) will consume this engine later.

## Scope

Included:

- `Lv1Command::SetGain` — new command variant on the existing actor.
- `fade` module: `FadeEngine` actor, fade types, curve math.
- `fade-test` CLI subcommand — real fade on a live LV1 channel.
- Automated tests: curve math, engine tick logic, override detection.

Excluded:

- Scene recall automation (Phase 7).
- Capture/listen mode (Phase 4).
- Storage (Phase 5).
- UI (Phase 6).
- Multi-fade concurrency — one active fade config at a time.
- Vegas/wave stress test — deferred, incompatible with single-fade design.

## Hardware Findings (Confirmed This Session)

From live testing against real LV1 hardware:

- `/Notify/Track/Out/Gain` always arrives with `T:true` on the 4th arg — for both surface moves and app-sent commands. `F:false` never appears.
- **Override detection must use value comparison**, not the flag.
- Echo latency: avg ~1ms at 25–60 Hz. LV1 handles 60 Hz comfortably on a single channel.
- Echo rate: 97.5–100% at all tested rates. No drops observed.
- **Safe update rate: 25 Hz confirmed. 40 Hz fine. 60 Hz works.**

## Architecture

Phase 3 adds one new module and one new command variant. Everything else is unchanged.

```
src/
  lv1/
    state.rs     (add SetGain to Lv1Command + handler)
  fade/
    mod.rs       (re-exports)
    engine.rs    (FadeEngineHandle, actor loop)
    curve.rs     (FadeCurve, interpolation)
  main.rs        (add fade-test subcommand)
```

### Responsibility Boundaries

- `Lv1Actor` — sole TCP owner and hardware adapter. Translates between OSC-over-TCP and typed Rust events. The only component that touches the network.
- `FadeEngine` — timed worker. Subscribes to `Lv1Event`, sends `Lv1Command::SetGain`. Knows nothing about scenes or capture.
- Future scene watcher (Phase 7) — decides when to call `StartFade` and `FinishNow` based on scene context.
- Future HTTP API (Phase 8) — also calls `FadeEngineHandle` directly.

There is no unified internal event bus in Phase 3. Each actor exposes a `subscribe()` method returning a typed event stream. A formal internal bus is deferred until Phase 7/8 when the need is concrete.

## `Lv1Command::SetGain`

New variant added to `src/lv1/state.rs`:

```rust
pub enum Lv1Command {
    GetState { reply: oneshot::Sender<Lv1StateSnapshot> },
    Subscribe { tx: mpsc::Sender<Lv1Event> },
    SetGain { group: i32, channel: i32, gain_db: f64 },  // new
}
```

The actor handles `SetGain` by sending `/Set/Track/Out/Gain` immediately via the existing TCP client. Fire and forget — no reply. If the TCP send fails, the existing disconnect/reconnect path handles it.

## Fade Types

```rust
// src/fade/curve.rs

pub enum FadeCurve {
    LinearDb,
    EaseInOutDb,
}

pub fn interpolate(start_db: f64, target_db: f64, t: f64, curve: FadeCurve) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let t_shaped = match curve {
        FadeCurve::LinearDb => t,
        FadeCurve::EaseInOutDb => t * t * (3.0 - 2.0 * t),  // smoothstep
    };
    start_db + (target_db - start_db) * t_shaped
}
```

```rust
// src/fade/engine.rs

pub struct FadeTarget {
    pub group: i32,
    pub channel: i32,
    pub target_db: f64,
}

pub struct FadeConfig {
    pub targets: Vec<FadeTarget>,
    pub duration_ms: u64,
    pub curve: FadeCurve,
}

pub enum FadeCommand {
    StartFade { config: FadeConfig },
    AbortAll,
    FinishNow,
}

pub enum FadeEvent {
    FadeStarted,
    FadeCompleted,
    FadeAborted,
    ChannelOverride { group: i32, channel: i32 },
    ChannelCancelled { group: i32, channel: i32 },
}

// Internal — owned exclusively by the engine task
struct ActiveChannel {
    group: i32,
    channel: i32,
    start_db: f64,      // live value at fade start, from get_state()
    target_db: f64,
    expected_db: f64,   // last value sent — for override detection
}
```

## Engine Actor Loop

The engine runs a `tokio::select!` loop with three arms:

### 1. Commands (`FadeCommand`)

**`StartFade { config }`:**
- If a fade is already running: cancel all active channels in place (no force-to-target), clear state.
- Call `lv1_handle.get_state()` to get current live fader values.
- Build `ActiveChannel` entries for each target found in the snapshot. Targets not found in the snapshot are skipped (channel may not exist).
- Start a fresh `tokio::time::interval` at 25 Hz.
- Fan out `FadeEvent::FadeStarted`.

**`AbortAll`:**
- Clear all active channels (no final sends).
- Drop the tick interval.
- Fan out `FadeEvent::FadeAborted`.

**`FinishNow`:**
- Force-send the exact `target_db` for every active channel via `SetGain`.
- Clear all active channels.
- Drop the tick interval.
- Fan out `FadeEvent::FadeCompleted`.
- **Completes within a single command dispatch** — no async wait. The scene watcher can immediately send a new `StartFade`.

### 2. 25 Hz Tick

Only active when a fade is running (interval exists).

For each active channel:
1. Compute `t = elapsed / duration` (clamped 0.0–1.0).
2. Compute `new_db = interpolate(start_db, target_db, t, curve)`.
3. If `|new_db - expected_db| >= 0.1 dB`: send `SetGain`, update `expected_db`.
4. If `t >= 1.0`: force-send exact `target_db`, mark channel done.

After processing all channels: if none remain, drop the interval, fan out `FadeEvent::FadeCompleted`.

### 3. Fader Events (`Lv1Event`)

The engine subscribes to `Lv1Event` from the actor at construction.

**`FaderChanged { group, channel, gain_db }`:**
- If channel is active and `|gain_db - expected_db| >= 0.5 dB`: override detected.
- Cancel that channel (Touch Cancels Channel — do not cancel others).
- Fan out `FadeEvent::ChannelOverride { group, channel }` then `FadeEvent::ChannelCancelled { group, channel }`.

**`Lv1Event::Disconnected`:**
- Abort all fades immediately (same as `AbortAll`).
- Fan out `FadeEvent::FadeAborted`.

All other `Lv1Event` variants are ignored.

## Constants

| Constant | Value | Reason |
|---|---|---|
| Tick rate | 25 Hz | Confirmed safe; smooth enough for musical fades |
| Min send delta | 0.1 dB | Avoids redundant sends when value hasn't changed meaningfully |
| Override threshold | 0.5 dB | Confirmed from PROJECT.md; distinguishes noise from intentional move |

## `FadeEngineHandle`

```rust
#[derive(Clone)]
pub struct FadeEngineHandle {
    tx: mpsc::Sender<FadeCommand>,
}

impl FadeEngineHandle {
    pub async fn start_fade(&self, config: FadeConfig) { ... }
    pub async fn abort_all(&self) { ... }
    pub async fn finish_now(&self) { ... }
    pub async fn subscribe(&self) -> mpsc::Receiver<FadeEvent> { ... }
}

pub fn spawn_engine(lv1: Lv1ActorHandle) -> FadeEngineHandle { ... }
```

Same pattern as `Lv1ActorHandle` — cloneable, async, channel-backed.

## `fade-test` CLI Subcommand

```
lv1-probe fade-test [--host <ip>] [--port <n>] --group <n> --channel <n>
                    --target-db <db> --duration-ms <n>
                    [--curve linear-db|ease-in-out-db]
```

1. Discover or connect to LV1.
2. Spawn `Lv1Actor`.
3. Spawn `FadeEngine`.
4. Subscribe to `FadeEvent`.
5. Send `StartFade` with the specified config.
6. Print events as they arrive (start, complete, abort, override — not per-tick to avoid noise).
7. Exit on `FadeCompleted` or `FadeAborted`.

Example output:
```
[connected] 192.168.1.10:50001
[fade-started] group=0 ch=0 -8.5 dB → -20.0 dB over 4000ms ease-in-out-db
[override] group=0 ch=0 — manual move detected, channel cancelled
[fade-complete] all channels reached target
```

## Testing

**Curve math (pure, no async):**
- `interpolate` at t=0.0 returns start, t=1.0 returns target.
- Ease-in-out has zero derivative at endpoints (verified numerically).
- Values are clamped — t outside 0.0–1.0 doesn't overshoot.

**Engine tick logic (pure `ActiveChannel` logic, no actor):**
- Channels complete at t≥1.0.
- Min delta suppresses redundant sends.
- Override detection triggers at ≥0.5 dB deviation.
- `StartFade` while running cancels previous channels in place.

**Actor integration (fake TCP server, same pattern as Phase 2):**
- Spawn actor + engine against fake server.
- Send `StartFade`, assert `SetGain` commands arrive at server.
- Simulate fader echo, assert no false overrides.
- Simulate unexpected fader value, assert `ChannelOverride` event.
- Send `AbortAll`, assert no further `SetGain` commands.
- Send `FinishNow`, assert exact target values sent.

## Open Questions for Phase 4

- Should `FadeEvent` include per-tick progress updates for the UI, or just start/complete/abort/override? (Deferred — no UI yet.)
- Should `StartFade` emit `FadeCompleted` for channels whose current live value is already at the target (within min delta)? Probably yes — avoids stale "in progress" state.

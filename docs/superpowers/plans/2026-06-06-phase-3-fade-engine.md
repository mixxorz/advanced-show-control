# Phase 3: Fade Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a fade engine tokio actor that animates LV1 faders from their current live positions to stored target values, with override detection, abort, and finish-now controls.

**Architecture:** `FadeEngine` is a separate tokio actor that subscribes to `Lv1Event` from the existing `Lv1Actor` and sends `Lv1Command::SetGain` commands back to it. The actor owns a 25 Hz tick loop while a fade is active. The `Lv1Actor` is the sole TCP owner — the fade engine never touches the network directly.

**Tech Stack:** Rust, tokio (`mpsc`, `oneshot`, `time::interval`, `time::Instant`), existing `lv1::state::Lv1ActorHandle`.

---

## File Map

| File | Status | Responsibility |
|---|---|---|
| `src/lv1/state.rs` | Modify | Add `SetGain` variant to `Lv1Command`, handle it in actor loop |
| `src/fade/mod.rs` | Create | Re-export `engine` and `curve` modules |
| `src/fade/curve.rs` | Create | `FadeCurve` enum, `interpolate` pure function |
| `src/fade/engine.rs` | Create | `FadeTarget`, `FadeConfig`, `FadeCommand`, `FadeEvent`, `ActiveChannel`, `FadeEngineHandle`, actor loop |
| `src/lib.rs` | Modify | Add `pub mod fade;` |
| `src/main.rs` | Modify | Add `fade-test` subcommand |

---

## Task 1: Add `SetGain` to `Lv1Command`

**Files:**
- Modify: `src/lv1/state.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src/lv1/state.rs`:

```rust
#[tokio::test]
async fn actor_handles_set_gain_command() {
    use crate::lv1::tcp::encode_frame;
    use std::net::TcpListener;
    use std::io::{Read, Write};

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.write_all(&encode_frame("/handshake", &[OscArg::Int(1)]).unwrap()).unwrap();
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test actor_handles_set_gain_command
```

Expected: FAIL — `set_gain` method not found on `Lv1ActorHandle`.

- [ ] **Step 3: Add `SetGain` variant to `Lv1Command`**

In `src/lv1/state.rs`, modify the `Lv1Command` enum:

```rust
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
```

- [ ] **Step 4: Add `set_gain` method to `Lv1ActorHandle`**

In `src/lv1/state.rs`, add to the `impl Lv1ActorHandle` block:

```rust
/// Send a `/Set/Track/Out/Gain` command to LV1. Fire and forget.
pub async fn set_gain(&self, group: i32, channel: i32, gain_db: f64) {
    let _ = self.tx.send(Lv1Command::SetGain { group, channel, gain_db }).await;
}
```

- [ ] **Step 5: Handle `SetGain` in the actor's command processing**

In `src/lv1/state.rs`, find the `run_connected` function. In the `cmd = cmd_rx.recv()` match arm, add the `SetGain` variant. The full match should look like:

```rust
Some(Lv1Command::GetState { reply }) => {
    let _ = reply.send(state.snapshot());
}
Some(Lv1Command::Subscribe { tx }) => {
    state.subscribers.push(tx);
}
Some(Lv1Command::SetGain { group, channel, gain_db }) => {
    let _ = client.send(
        "/Set/Track/Out/Gain",
        &[
            crate::osc::OscArg::Int(group),
            crate::osc::OscArg::Int(channel),
            crate::osc::OscArg::Double(gain_db),
        ],
    );
}
```

Also handle it in `drain_commands_for` (used during reconnect delays) — `SetGain` during disconnect is silently dropped:

```rust
Some(Lv1Command::GetState { reply }) => {
    let _ = reply.send(state.snapshot());
}
Some(Lv1Command::Subscribe { tx }) => {
    state.subscribers.push(tx);
}
Some(Lv1Command::SetGain { .. }) => {
    // Silently drop — not connected
}
```

- [ ] **Step 6: Run test to verify it passes**

```bash
cargo test actor_handles_set_gain_command
```

Expected: PASS.

- [ ] **Step 7: Run full test suite to confirm nothing is broken**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/lv1/state.rs
git commit -m "feat: add SetGain command to Lv1Actor"
```

---

## Task 2: Curve math

**Files:**
- Create: `src/fade/mod.rs`
- Create: `src/fade/curve.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create `src/fade/mod.rs`**

```rust
pub mod curve;
pub mod engine;
```

- [ ] **Step 2: Add `pub mod fade;` to `src/lib.rs`**

Open `src/lib.rs` and add:

```rust
pub mod fade;
pub mod lv1;
pub mod osc;
```

(Keep existing module declarations, just add the new one.)

- [ ] **Step 3: Write the failing tests**

Create `src/fade/curve.rs` with tests only (no implementation yet):

```rust
//! Fade curve interpolation.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FadeCurve {
    LinearDb,
    EaseInOutDb,
}

pub fn interpolate(start_db: f64, target_db: f64, t: f64, curve: FadeCurve) -> f64 {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_db_at_t0_returns_start() {
        assert_eq!(interpolate(-20.0, -10.0, 0.0, FadeCurve::LinearDb), -20.0);
    }

    #[test]
    fn linear_db_at_t1_returns_target() {
        assert_eq!(interpolate(-20.0, -10.0, 1.0, FadeCurve::LinearDb), -10.0);
    }

    #[test]
    fn linear_db_at_midpoint_is_halfway() {
        let v = interpolate(-20.0, -10.0, 0.5, FadeCurve::LinearDb);
        assert!((v - -15.0).abs() < 1e-10);
    }

    #[test]
    fn ease_in_out_at_t0_returns_start() {
        assert_eq!(interpolate(-20.0, -10.0, 0.0, FadeCurve::EaseInOutDb), -20.0);
    }

    #[test]
    fn ease_in_out_at_t1_returns_target() {
        assert_eq!(interpolate(-20.0, -10.0, 1.0, FadeCurve::EaseInOutDb), -10.0);
    }

    #[test]
    fn ease_in_out_at_midpoint_is_halfway() {
        // smoothstep(0.5) = 0.5 * 0.5 * (3 - 2*0.5) = 0.25 * 2 = 0.5
        let v = interpolate(-20.0, -10.0, 0.5, FadeCurve::EaseInOutDb);
        assert!((v - -15.0).abs() < 1e-10);
    }

    #[test]
    fn ease_in_out_has_slow_start() {
        // At t=0.1, ease-in-out should be less than linear
        let linear = interpolate(0.0, 1.0, 0.1, FadeCurve::LinearDb);
        let eased = interpolate(0.0, 1.0, 0.1, FadeCurve::EaseInOutDb);
        assert!(eased < linear);
    }

    #[test]
    fn ease_in_out_has_slow_end() {
        // At t=0.9, ease-in-out should be greater than linear (closer to target)
        let linear = interpolate(0.0, 1.0, 0.9, FadeCurve::LinearDb);
        let eased = interpolate(0.0, 1.0, 0.9, FadeCurve::EaseInOutDb);
        assert!(eased > linear);
    }

    #[test]
    fn clamps_t_below_zero() {
        assert_eq!(interpolate(-20.0, -10.0, -1.0, FadeCurve::LinearDb), -20.0);
    }

    #[test]
    fn clamps_t_above_one() {
        assert_eq!(interpolate(-20.0, -10.0, 2.0, FadeCurve::LinearDb), -10.0);
    }

    #[test]
    fn works_with_fade_up() {
        let v = interpolate(-30.0, -10.0, 0.5, FadeCurve::LinearDb);
        assert!((v - -20.0).abs() < 1e-10);
    }

    #[test]
    fn works_with_fade_down() {
        let v = interpolate(-10.0, -30.0, 0.5, FadeCurve::LinearDb);
        assert!((v - -20.0).abs() < 1e-10);
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

```bash
cargo test --lib fade::curve
```

Expected: FAIL — `todo!()` panics.

- [ ] **Step 5: Implement `interpolate`**

Replace the `todo!()` stub in `src/fade/curve.rs`:

```rust
pub fn interpolate(start_db: f64, target_db: f64, t: f64, curve: FadeCurve) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let t_shaped = match curve {
        FadeCurve::LinearDb => t,
        FadeCurve::EaseInOutDb => t * t * (3.0 - 2.0 * t),
    };
    start_db + (target_db - start_db) * t_shaped
}
```

- [ ] **Step 6: Run tests to verify they pass**

```bash
cargo test --lib fade::curve
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add src/fade/mod.rs src/fade/curve.rs src/lib.rs
git commit -m "feat: add fade curve interpolation"
```

---

## Task 3: Fade engine types and pure tick logic

**Files:**
- Create: `src/fade/engine.rs`

- [ ] **Step 1: Write the failing tests**

Create `src/fade/engine.rs` with the types and tests (no actor yet):

```rust
//! Fade engine actor — animates LV1 faders over time.

use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::fade::curve::{FadeCurve, interpolate};
use crate::lv1::state::Lv1ActorHandle;

pub const TICK_HZ: u64 = 25;
pub const MIN_SEND_DELTA_DB: f64 = 0.1;
pub const OVERRIDE_THRESHOLD_DB: f64 = 0.5;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FadeTarget {
    pub group: i32,
    pub channel: i32,
    pub target_db: f64,
}

#[derive(Debug, Clone)]
pub struct FadeConfig {
    pub targets: Vec<FadeTarget>,
    pub duration_ms: u64,
    pub curve: FadeCurve,
}

pub enum FadeCommand {
    StartFade { config: FadeConfig },
    AbortAll,
    FinishNow,
    Subscribe { tx: mpsc::Sender<FadeEvent> },
}

#[derive(Debug, Clone)]
pub enum FadeEvent {
    FadeStarted,
    FadeCompleted,
    FadeAborted,
    ChannelOverride { group: i32, channel: i32 },
    ChannelCancelled { group: i32, channel: i32 },
}

// ---------------------------------------------------------------------------
// Internal tick logic (pure — no async, easy to test)
// ---------------------------------------------------------------------------

pub(crate) struct ActiveChannel {
    pub group: i32,
    pub channel: i32,
    pub start_db: f64,
    pub target_db: f64,
    pub expected_db: f64,
    pub curve: FadeCurve,
    pub duration: Duration,
    pub started_at: Instant,
}

impl ActiveChannel {
    pub(crate) fn new(
        group: i32,
        channel: i32,
        start_db: f64,
        target_db: f64,
        curve: FadeCurve,
        duration: Duration,
        started_at: Instant,
    ) -> Self {
        Self {
            group,
            channel,
            start_db,
            target_db,
            expected_db: start_db,
            curve,
            duration,
            started_at,
        }
    }

    /// Returns the interpolated dB value at `now`.
    pub(crate) fn value_at(&self, now: Instant) -> f64 {
        let elapsed = now.duration_since(self.started_at).as_secs_f64();
        let t = elapsed / self.duration.as_secs_f64();
        interpolate(self.start_db, self.target_db, t, self.curve)
    }

    /// Returns true if the fade has completed (t >= 1.0).
    pub(crate) fn is_done(&self, now: Instant) -> bool {
        now.duration_since(self.started_at) >= self.duration
    }

    /// Returns true if `reported` deviates from `expected_db` by >= threshold.
    pub(crate) fn is_override(&self, reported_db: f64) -> bool {
        (reported_db - self.expected_db).abs() >= OVERRIDE_THRESHOLD_DB
    }

    /// Returns Some(new_db) if the value has moved enough to warrant sending.
    pub(crate) fn next_send(&mut self, now: Instant) -> Option<f64> {
        let new_db = if self.is_done(now) {
            self.target_db
        } else {
            self.value_at(now)
        };

        if (new_db - self.expected_db).abs() >= MIN_SEND_DELTA_DB {
            self.expected_db = new_db;
            Some(new_db)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_channel(start_db: f64, target_db: f64, duration_ms: u64) -> ActiveChannel {
        ActiveChannel::new(
            0, 0, start_db, target_db,
            FadeCurve::LinearDb,
            Duration::from_millis(duration_ms),
            Instant::now(),
        )
    }

    #[test]
    fn value_at_start_is_start_db() {
        let ch = make_channel(-20.0, -10.0, 4000);
        let v = ch.value_at(ch.started_at);
        assert!((v - -20.0).abs() < 1e-10);
    }

    #[test]
    fn value_at_end_is_target_db() {
        let ch = make_channel(-20.0, -10.0, 4000);
        let end = ch.started_at + Duration::from_millis(4000);
        let v = ch.value_at(end);
        assert!((v - -10.0).abs() < 1e-10);
    }

    #[test]
    fn is_done_false_before_duration() {
        let ch = make_channel(-20.0, -10.0, 4000);
        let mid = ch.started_at + Duration::from_millis(2000);
        assert!(!ch.is_done(mid));
    }

    #[test]
    fn is_done_true_at_duration() {
        let ch = make_channel(-20.0, -10.0, 4000);
        let end = ch.started_at + Duration::from_millis(4000);
        assert!(ch.is_done(end));
    }

    #[test]
    fn is_override_true_when_deviation_exceeds_threshold() {
        let ch = make_channel(-20.0, -10.0, 4000);
        // expected_db starts at start_db (-20.0)
        assert!(ch.is_override(-20.0 + OVERRIDE_THRESHOLD_DB + 0.1));
    }

    #[test]
    fn is_override_false_when_deviation_below_threshold() {
        let ch = make_channel(-20.0, -10.0, 4000);
        assert!(!ch.is_override(-20.0 + OVERRIDE_THRESHOLD_DB - 0.1));
    }

    #[test]
    fn next_send_returns_none_when_below_min_delta() {
        let mut ch = make_channel(-20.0, -10.0, 4000);
        // At t=0, value is -20.0 = expected_db, delta is 0 — no send
        let now = ch.started_at;
        assert!(ch.next_send(now).is_none());
    }

    #[test]
    fn next_send_returns_some_when_above_min_delta() {
        let mut ch = make_channel(-20.0, -10.0, 4000);
        // At t=1.0 (end), value is -10.0, delta from -20.0 is 10.0 >= 0.1
        let end = ch.started_at + Duration::from_millis(4000);
        let result = ch.next_send(end);
        assert!(result.is_some());
        assert!((result.unwrap() - -10.0).abs() < 1e-10);
    }

    #[test]
    fn next_send_updates_expected_db() {
        let mut ch = make_channel(-20.0, -10.0, 4000);
        let end = ch.started_at + Duration::from_millis(4000);
        ch.next_send(end);
        assert!((ch.expected_db - -10.0).abs() < 1e-10);
    }

    #[test]
    fn next_send_at_done_returns_exact_target() {
        let mut ch = make_channel(-20.0, -10.0, 4000);
        let end = ch.started_at + Duration::from_millis(5000);
        let result = ch.next_send(end).unwrap();
        assert!((result - -10.0).abs() < 1e-10);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

```bash
cargo test --lib fade::engine
```

Expected: all pass (no actor yet, just pure logic).

- [ ] **Step 3: Commit**

```bash
git add src/fade/engine.rs
git commit -m "feat: add fade engine types and tick logic"
```

---

## Task 4: Fade engine actor

**Files:**
- Modify: `src/fade/engine.rs`

- [ ] **Step 1: Write the failing integration tests**

Add to the `tests` module in `src/fade/engine.rs`:

```rust
    use crate::lv1::state::{Lv1Event, spawn_actor};
    use crate::lv1::tcp::encode_frame;
    use crate::osc::OscArg;
    use std::io::Write;
    use std::net::TcpListener;

    fn lv1_frame(address: &str, args: &[OscArg]) -> Vec<u8> {
        encode_frame(address, args).unwrap()
    }

    async fn wait_for_fade_event(
        events: &mut mpsc::Receiver<FadeEvent>,
        timeout: std::time::Duration,
        pred: impl Fn(&FadeEvent) -> bool,
    ) -> FadeEvent {
        tokio::time::timeout(timeout, async {
            while let Some(e) = events.recv().await {
                if pred(&e) { return e; }
            }
            panic!("event stream ended without matching event");
        }).await.expect("timed out waiting for fade event")
    }

    #[tokio::test]
    async fn engine_emits_fade_started_and_completed() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
            // Send a /Channels batch with one channel so start values are available
            let channels_args = {
                let mut a = vec![OscArg::Int(1)];
                a.push(OscArg::String("Ch 1".to_string()));
                a.push(OscArg::Int(0)); // group
                a.push(OscArg::Int(0)); // channel
                a.push(OscArg::Double(-8.0)); // gain_db
                for _ in 0..15 { a.push(OscArg::Int(0)); }
                a
            };
            stream.write_all(&lv1_frame("/Channels", &channels_args)).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(3));
        });

        let lv1 = spawn_actor("127.0.0.1".to_string(), port);
        let engine = spawn_engine(lv1);
        let mut fade_events = engine.subscribe().await;

        // Wait a moment for /Channels to be processed
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        engine.start_fade(FadeConfig {
            targets: vec![FadeTarget { group: 0, channel: 0, target_db: -10.0 }],
            duration_ms: 500,
            curve: FadeCurve::LinearDb,
        }).await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_millis(500),
            |e| matches!(e, FadeEvent::FadeStarted),
        ).await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_secs(3),
            |e| matches!(e, FadeEvent::FadeCompleted),
        ).await;
    }

    #[tokio::test]
    async fn engine_abort_all_stops_fade() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
            let channels_args = {
                let mut a = vec![OscArg::Int(1)];
                a.push(OscArg::String("Ch 1".to_string()));
                a.push(OscArg::Int(0));
                a.push(OscArg::Int(0));
                a.push(OscArg::Double(-8.0));
                for _ in 0..15 { a.push(OscArg::Int(0)); }
                a
            };
            stream.write_all(&lv1_frame("/Channels", &channels_args)).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(5));
        });

        let lv1 = spawn_actor("127.0.0.1".to_string(), port);
        let engine = spawn_engine(lv1);
        let mut fade_events = engine.subscribe().await;

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        engine.start_fade(FadeConfig {
            targets: vec![FadeTarget { group: 0, channel: 0, target_db: -30.0 }],
            duration_ms: 10_000,
            curve: FadeCurve::LinearDb,
        }).await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_millis(500),
            |e| matches!(e, FadeEvent::FadeStarted),
        ).await;

        engine.abort_all().await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_secs(2),
            |e| matches!(e, FadeEvent::FadeAborted),
        ).await;
    }

    #[tokio::test]
    async fn engine_detects_manual_override() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
            let channels_args = {
                let mut a = vec![OscArg::Int(1)];
                a.push(OscArg::String("Ch 1".to_string()));
                a.push(OscArg::Int(0));
                a.push(OscArg::Int(0));
                a.push(OscArg::Double(-8.0));
                for _ in 0..15 { a.push(OscArg::Int(0)); }
                a
            };
            stream.write_all(&lv1_frame("/Channels", &channels_args)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(400));

            // Simulate a large unexpected fader move (override)
            stream.write_all(&lv1_frame(
                "/Notify/Track/Out/Gain",
                &[OscArg::Int(0), OscArg::Int(0), OscArg::Double(0.0), OscArg::True],
            )).unwrap();

            std::thread::sleep(std::time::Duration::from_secs(3));
        });

        let lv1 = spawn_actor("127.0.0.1".to_string(), port);
        let engine = spawn_engine(lv1);
        let mut fade_events = engine.subscribe().await;

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        engine.start_fade(FadeConfig {
            targets: vec![FadeTarget { group: 0, channel: 0, target_db: -20.0 }],
            duration_ms: 10_000,
            curve: FadeCurve::LinearDb,
        }).await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_millis(500),
            |e| matches!(e, FadeEvent::FadeStarted),
        ).await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_secs(3),
            |e| matches!(e, FadeEvent::ChannelOverride { .. }),
        ).await;
    }

    #[tokio::test]
    async fn start_fade_while_running_replaces_previous() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
            let channels_args = {
                let mut a = vec![OscArg::Int(1)];
                a.push(OscArg::String("Ch 1".to_string()));
                a.push(OscArg::Int(0));
                a.push(OscArg::Int(0));
                a.push(OscArg::Double(-8.0));
                for _ in 0..15 { a.push(OscArg::Int(0)); }
                a
            };
            stream.write_all(&lv1_frame("/Channels", &channels_args)).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(5));
        });

        let lv1 = spawn_actor("127.0.0.1".to_string(), port);
        let engine = spawn_engine(lv1);
        let mut fade_events = engine.subscribe().await;

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // First fade — very long
        engine.start_fade(FadeConfig {
            targets: vec![FadeTarget { group: 0, channel: 0, target_db: -30.0 }],
            duration_ms: 30_000,
            curve: FadeCurve::LinearDb,
        }).await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_millis(500),
            |e| matches!(e, FadeEvent::FadeStarted),
        ).await;

        // Second fade — short, replaces first
        engine.start_fade(FadeConfig {
            targets: vec![FadeTarget { group: 0, channel: 0, target_db: -10.0 }],
            duration_ms: 500,
            curve: FadeCurve::LinearDb,
        }).await;

        // Should get another FadeStarted (for the second fade)
        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_millis(500),
            |e| matches!(e, FadeEvent::FadeStarted),
        ).await;

        // Should complete the second fade
        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_secs(3),
            |e| matches!(e, FadeEvent::FadeCompleted),
        ).await;
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --lib fade::engine::tests::engine_emits_fade_started_and_completed
```

Expected: FAIL — `spawn_engine` not defined.

- [ ] **Step 3: Implement the fade engine actor**

Add to `src/fade/engine.rs` (before `#[cfg(test)]`):

```rust
// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FadeEngineHandle {
    tx: mpsc::Sender<FadeCommand>,
}

impl FadeEngineHandle {
    pub async fn start_fade(&self, config: FadeConfig) {
        let _ = self.tx.send(FadeCommand::StartFade { config }).await;
    }

    pub async fn abort_all(&self) {
        let _ = self.tx.send(FadeCommand::AbortAll).await;
    }

    pub async fn finish_now(&self) {
        let _ = self.tx.send(FadeCommand::FinishNow).await;
    }

    pub async fn subscribe(&self) -> mpsc::Receiver<FadeEvent> {
        let (tx, rx) = mpsc::channel(64);
        let _ = self.tx.send(FadeCommand::Subscribe { tx }).await;
        rx
    }
}

pub fn spawn_engine(lv1: Lv1ActorHandle) -> FadeEngineHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    tokio::spawn(run_engine(lv1, cmd_rx));
    FadeEngineHandle { tx: cmd_tx }
}

// ---------------------------------------------------------------------------
// Actor internals
// ---------------------------------------------------------------------------

struct EngineState {
    channels: Vec<ActiveChannel>,
    curve: FadeCurve,
    duration: Duration,
    subscribers: Vec<mpsc::Sender<FadeEvent>>,
}

impl EngineState {
    fn new() -> Self {
        Self {
            channels: Vec::new(),
            curve: FadeCurve::EaseInOutDb,
            duration: Duration::from_secs(4),
            subscribers: Vec::new(),
        }
    }

    fn fan_out(&mut self, event: FadeEvent) {
        self.subscribers.retain(|tx| tx.try_send(event.clone()).is_ok());
    }

    fn is_active(&self) -> bool {
        !self.channels.is_empty()
    }

    fn cancel_all_in_place(&mut self) {
        self.channels.clear();
    }
}

async fn run_engine(lv1: Lv1ActorHandle, mut cmd_rx: mpsc::Receiver<FadeCommand>) {
    let mut lv1_events = lv1.subscribe().await;
    let mut state = EngineState::new();
    let mut tick_interval: Option<tokio::time::Interval> = None;

    loop {
        // Build the tick future: only poll when active
        let tick_fut = async {
            match tick_interval.as_mut() {
                Some(interval) => { interval.tick().await; true }
                None => { std::future::pending::<bool>().await }
            }
        };

        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    None => break,
                    Some(FadeCommand::Subscribe { tx }) => {
                        state.subscribers.push(tx);
                    }
                    Some(FadeCommand::StartFade { config }) => {
                        state.cancel_all_in_place();

                        let snapshot = lv1.get_state().await;
                        let now = Instant::now();
                        let duration = Duration::from_millis(config.duration_ms);

                        for target in &config.targets {
                            let start_db = snapshot.channels.iter()
                                .find(|ch| ch.group == target.group && ch.channel == target.channel)
                                .map(|ch| ch.gain_db)
                                .unwrap_or(target.target_db); // skip unknowns by fading from target (no-op)

                            state.channels.push(ActiveChannel::new(
                                target.group,
                                target.channel,
                                start_db,
                                target.target_db,
                                config.curve,
                                duration,
                                now,
                            ));
                        }

                        state.curve = config.curve;
                        state.duration = duration;
                        let mut interval = tokio::time::interval(Duration::from_millis(1000 / TICK_HZ));
                        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                        tick_interval = Some(interval);

                        state.fan_out(FadeEvent::FadeStarted);
                    }
                    Some(FadeCommand::AbortAll) => {
                        state.cancel_all_in_place();
                        tick_interval = None;
                        state.fan_out(FadeEvent::FadeAborted);
                    }
                    Some(FadeCommand::FinishNow) => {
                        for ch in &state.channels {
                            lv1.set_gain(ch.group, ch.channel, ch.target_db).await;
                        }
                        state.cancel_all_in_place();
                        tick_interval = None;
                        state.fan_out(FadeEvent::FadeCompleted);
                    }
                }
            }

            _ = tick_fut => {
                let now = Instant::now();
                let mut done_indices = Vec::new();

                for (i, ch) in state.channels.iter_mut().enumerate() {
                    if let Some(new_db) = ch.next_send(now) {
                        lv1.set_gain(ch.group, ch.channel, new_db).await;
                    }
                    if ch.is_done(now) {
                        done_indices.push(i);
                    }
                }

                // Remove completed channels (reverse order to preserve indices)
                for i in done_indices.into_iter().rev() {
                    state.channels.remove(i);
                }

                if !state.is_active() {
                    tick_interval = None;
                    fade_start = None;
                    state.fan_out(FadeEvent::FadeCompleted);
                }
            }

            lv1_event = lv1_events.recv() => {
                match lv1_event {
                    Some(crate::lv1::state::Lv1Event::FaderChanged { group, channel, gain_db }) => {
                        if let Some(pos) = state.channels.iter().position(|ch| ch.group == group && ch.channel == channel) {
                            if state.channels[pos].is_override(gain_db) {
                                state.fan_out(FadeEvent::ChannelOverride { group, channel });
                                state.channels.remove(pos);
                                state.fan_out(FadeEvent::ChannelCancelled { group, channel });

                                if !state.is_active() {
                                    tick_interval = None;
                                    fade_start = None;
                                }
                            }
                        }
                    }
                    Some(crate::lv1::state::Lv1Event::Disconnected) => {
                        if state.is_active() {
                            state.cancel_all_in_place();
                            tick_interval = None;
                            fade_start = None;
                            state.fan_out(FadeEvent::FadeAborted);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
```

- [ ] **Step 4: Run integration tests**

```bash
cargo test --lib fade::engine::tests::engine_emits_fade_started_and_completed
cargo test --lib fade::engine::tests::engine_abort_all_stops_fade
cargo test --lib fade::engine::tests::engine_detects_manual_override
cargo test --lib fade::engine::tests::start_fade_while_running_replaces_previous
```

Expected: all pass.

- [ ] **Step 5: Run full test suite**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/fade/engine.rs
git commit -m "feat: add fade engine actor"
```

---

## Task 5: `fade-test` CLI subcommand

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add the `FadeTest` variant to the `Command` enum**

In `src/main.rs`, add to the `Command` enum (after `RateTest`):

```rust
    #[command(about = "Run a timed fade on a single LV1 channel")]
    FadeTest {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long, default_value_t = 6000)]
        timeout_ms: u64,
        #[arg(long, default_value_t = 0)]
        group: i32,
        #[arg(long, default_value_t = 0)]
        channel: i32,
        #[arg(long, allow_hyphen_values = true)]
        target_db: f64,
        #[arg(long, default_value_t = 4000)]
        duration_ms: u64,
        #[arg(long, value_enum, default_value_t = CurveArg::EaseInOutDb)]
        curve: CurveArg,
    },
```

- [ ] **Step 2: Add the `CurveArg` clap enum**

Add above the `Cli` struct in `src/main.rs`:

```rust
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum CurveArg {
    LinearDb,
    EaseInOutDb,
}
```

- [ ] **Step 3: Add the match arm in `main`**

In `src/main.rs`, add to the `match cli.command` block:

```rust
        Command::FadeTest {
            host, port, timeout_ms, group, channel, target_db, duration_ms, curve,
        } => run_fade_test(host, port, timeout_ms, group, channel, target_db, duration_ms, curve),
```

- [ ] **Step 4: Add the `run_fade_test` function**

Add to `src/main.rs`:

```rust
fn run_fade_test(
    host: Option<String>,
    port: Option<u16>,
    timeout_ms: u64,
    group: i32,
    channel: i32,
    target_db: f64,
    duration_ms: u64,
    curve: CurveArg,
) -> Result<(), Box<dyn std::error::Error>> {
    use lv1_scene_fade_utility::fade::curve::FadeCurve;
    use lv1_scene_fade_utility::fade::engine::{FadeConfig, FadeEvent, FadeTarget, spawn_engine};
    use lv1_scene_fade_utility::lv1::state::spawn_actor;

    let (host, port) = resolve_target(host, port, timeout_ms)?;
    eprintln!("connecting to {host}:{port}");

    let fade_curve = match curve {
        CurveArg::LinearDb => FadeCurve::LinearDb,
        CurveArg::EaseInOutDb => FadeCurve::EaseInOutDb,
    };

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let lv1 = spawn_actor(host.clone(), port);
        let engine = spawn_engine(lv1.clone());
        let mut lv1_events = lv1.subscribe().await;
        let mut fade_events = engine.subscribe().await;

        // Wait for LV1 connection
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while let Some(e) = lv1_events.recv().await {
                if matches!(e, lv1_scene_fade_utility::lv1::state::Lv1Event::Connected) {
                    println!("[connected] {host}:{port}");
                    break;
                }
            }
        }).await.map_err(|_| "timed out waiting for LV1 connection")?;

        // Wait briefly for /Channels to arrive
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let snapshot = lv1.get_state().await;
        let current_db = snapshot.channels.iter()
            .find(|ch| ch.group == group && ch.channel == channel)
            .map(|ch| ch.gain_db);

        match current_db {
            Some(db) => println!("[current] group={group} ch={channel} {db:.1} dB → {target_db:.1} dB over {duration_ms}ms {:?}", fade_curve),
            None => println!("[warning] channel group={group} ch={channel} not found in snapshot — fade will start from target"),
        }

        engine.start_fade(FadeConfig {
            targets: vec![FadeTarget { group, channel, target_db }],
            duration_ms,
            curve: fade_curve,
        }).await;

        loop {
            match fade_events.recv().await {
                Some(FadeEvent::FadeStarted) => println!("[fade-started]"),
                Some(FadeEvent::FadeCompleted) => { println!("[fade-complete] reached {target_db:.1} dB"); break; }
                Some(FadeEvent::FadeAborted) => { println!("[fade-aborted]"); break; }
                Some(FadeEvent::ChannelOverride { group, channel }) => {
                    println!("[override] group={group} ch={channel} — manual move detected, channel cancelled");
                }
                Some(FadeEvent::ChannelCancelled { group, channel }) => {
                    println!("[cancelled] group={group} ch={channel}");
                }
                None => break,
            }
        }

        Ok::<(), &str>(())
    })?;

    Ok(())
}
```

- [ ] **Step 5: Add CLI parse test**

Add to the `tests` module in `src/main.rs`:

```rust
    #[test]
    fn parses_fade_test_command() {
        let cli = Cli::try_parse_from([
            "lv1-probe",
            "fade-test",
            "--host", "192.168.1.10",
            "--port", "50001",
            "--group", "0",
            "--channel", "2",
            "--target-db", "-20.0",
            "--duration-ms", "3000",
            "--curve", "linear-db",
        ]).unwrap();

        match cli.command {
            Command::FadeTest { host, port, group, channel, target_db, duration_ms, curve, .. } => {
                assert_eq!(host.as_deref(), Some("192.168.1.10"));
                assert_eq!(port, Some(50001));
                assert_eq!(group, 0);
                assert_eq!(channel, 2);
                assert!((target_db - -20.0).abs() < 1e-10);
                assert_eq!(duration_ms, 3000);
                assert!(matches!(curve, CurveArg::LinearDb));
            }
            other => panic!("expected FadeTest, got {other:?}"),
        }
    }
```

- [ ] **Step 6: Run the CLI test**

```bash
cargo test parses_fade_test_command
```

Expected: PASS.

- [ ] **Step 7: Build and smoke-test the help**

```bash
cargo build --release
./target/release/lv1-scene-fade-utility fade-test --help
```

Expected: help text showing all options.

- [ ] **Step 8: Run full test suite**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/main.rs
git commit -m "feat: add fade-test CLI subcommand"
```

---

## Task 6: Full test suite and smoke test on hardware

- [ ] **Step 1: Run complete test suite**

```bash
cargo test
```

Expected: all tests pass, no warnings on new code.

- [ ] **Step 2: Smoke test on real hardware**

With LV1 on the network:

```bash
# Fade CH 1 from current position to -20 dB over 4 seconds
./target/release/lv1-scene-fade-utility fade-test --group 0 --channel 0 --target-db -20.0 --duration-ms 4000 --curve ease-in-out-db
```

Expected output:
```
connecting to 192.168.1.x:50001
[connected] 192.168.1.x:50001
[current] group=0 ch=0 0.0 dB → -20.0 dB over 4000ms EaseInOutDb
[fade-started]
[fade-complete] reached -20.0 dB
```

- [ ] **Step 3: Test manual override mid-fade**

```bash
# Start a long fade, then move the fader manually during it
./target/release/lv1-scene-fade-utility fade-test --group 0 --channel 0 --target-db -30.0 --duration-ms 20000
# Move the fader by hand — should see [override] then [fade-complete] or channel just stops
```

- [ ] **Step 4: Reset CH 1 to 0 dB**

```bash
./target/release/lv1-scene-fade-utility set-gain --group 0 --channel 0 --gain-db 0.0
```

- [ ] **Step 5: Final commit if any fixes were needed**

```bash
git add -p
git commit -m "fix: address issues found during hardware smoke test"
```

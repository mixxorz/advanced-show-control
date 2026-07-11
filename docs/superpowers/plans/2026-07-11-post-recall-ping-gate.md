# Post-Recall Ping Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pause all timed fade writes after a validated LV1 scene recall until two later same-generation keepalive pings show that LV1 activity has resumed.

**Architecture:** `Lv1Actor` will sequence accepted pings, expose the current sequence in fresh snapshots, and publish generation-tagged `PingReceived` facts. `FadeEngine` will own one generation-wide readiness barrier, pause every active target, count two pings newer than the snapshot boundary, and rebase target timelines on release; timeout and lifecycle failures abort safely.

**Tech Stack:** Rust, Tokio actors and time, `AppEventBus`, `tracing`, Cargo nextest.

## Global Constraints

- Treat real-hardware ping delay during scene recall as an explicit unconfirmed assumption; the current simulator continues its approximately 200 ms cadence during recalls.
- Require exactly two pings after the latest validated recall.
- Use a fixed five-second readiness timeout.
- Keep zero-duration recall behavior unchanged.
- Preserve current parameter-key overlap/replacement behavior; scene-owned same-scene finishing is separately tracked by GitHub issue #42.
- Blocked, skipped, disabled, stale, or unsafe recalls must not alter active fades or the readiness barrier.
- Never send parameter writes after disconnect, generation change, unavailable state, timeout, or Abort All.
- Use approved Rust test styles only: pure unit tests for target timing and actor tests through mailboxes, `AppEventBus`, TCP fixtures, and tracing listeners.
- GitHub issue #38 is optional follow-up test infrastructure and is not a dependency.

---

## File Map

- `src-tauri/src/lv1/events.rs`: define the public `PingReceived` runtime fact.
- `src-tauri/src/lv1/types.rs`: include the accepted ping sequence in `Lv1StateSnapshot`.
- `src-tauri/src/lv1/state.rs`: own, increment, and snapshot the connection-local ping sequence.
- `src-tauri/src/lv1/actor.rs`: publish a ping fact after a valid ping is accepted and its pong is queued.
- `src-tauri/tests/lv1_actor.rs`: actor/TCP coverage for pong preservation, sequence increments, and snapshots.
- `src-tauri/src/fade/tick.rs`: pause and rebase target timelines without advancing interpolation.
- `src-tauri/src/fade/state.rs`: own readiness barrier state and its pure transitions.
- `src-tauri/src/fade/actor.rs`: establish/reset the barrier, suppress writes, release on pings, and abort on timeout/lifecycle changes.
- `docs/architecture.md`: document the ping fact and generation-wide fade readiness barrier.

### Task 1: Publish Sequenced LV1 Ping Facts

**Files:**
- Modify: `src-tauri/src/lv1/events.rs`
- Modify: `src-tauri/src/lv1/types.rs`
- Modify: `src-tauri/src/lv1/state.rs`
- Modify: `src-tauri/src/lv1/actor.rs:347-370`
- Test: `src-tauri/tests/lv1_actor.rs`

**Interfaces:**
- Produces: `Lv1Event::PingReceived { sequence: u64 }`.
- Produces: `Lv1StateSnapshot::ping_sequence: u64`.
- Preserves: `/ping` arguments are returned unchanged in `/pong`.

- [ ] **Step 1: Write the failing LV1 actor test**

Add a sibling to `actor_routes_pong_without_blocking_read_loop` that subscribes before connection, sends two pings from the fake server, reads both pongs, requests `Lv1Command::GetState`, and asserts:

```rust
assert!(matches!(
    first_ping_event,
    AppEvent::Lv1 {
        generation: 7,
        event: Lv1Event::PingReceived { sequence: 1 },
    }
));
assert!(matches!(
    second_ping_event,
    AppEvent::Lv1 {
        generation: 7,
        event: Lv1Event::PingReceived { sequence: 2 },
    }
));
assert_eq!(snapshot.ping_sequence, 2);
assert_eq!(first_pong_args, first_ping_args);
assert_eq!(second_pong_args, second_ping_args);
```

Filter unrelated handshake/topology events in the receiver loop rather than assuming the ping fact is the next event.

- [ ] **Step 2: Run the targeted test and verify RED**

Run: `cargo nextest run -p advanced-show-control ping_sequence`

Expected: compilation fails because `PingReceived` and `ping_sequence` do not exist.

- [ ] **Step 3: Add the ping event and snapshot field**

Add to `Lv1Event`:

```rust
PingReceived {
    sequence: u64,
},
```

Add to `Lv1StateSnapshot` and `ActorState`:

```rust
pub ping_sequence: u64,
```

Initialize `ActorState::ping_sequence` to `0` and copy it from `ActorState::snapshot()`.

- [ ] **Step 4: Sequence accepted pings and publish after pong enqueue**

In the existing `pong_for_ping` branch, after `enqueue_writer_bytes` succeeds and before `continue`, add:

```rust
state.ping_sequence = state.ping_sequence.saturating_add(1);
state.fan_out(Lv1Event::PingReceived {
    sequence: state.ping_sequence,
});
state.last_ping = Instant::now();
```

Do not add an `INFO`/`WARN` log or route ping facts through the projector.

- [ ] **Step 5: Update all snapshot literals**

Use compiler errors to add `ping_sequence: 0` to test-owned `Lv1StateSnapshot` literals. Use controlled nonzero values only in ping-boundary tests added later.

- [ ] **Step 6: Run targeted LV1 tests and verify GREEN**

Run: `cargo nextest run -p advanced-show-control lv1_actor`

Expected: all LV1 actor tests pass, including two matching pongs, sequences `1` and `2`, and snapshot sequence `2`.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/lv1/events.rs src-tauri/src/lv1/types.rs src-tauri/src/lv1/state.rs src-tauri/src/lv1/actor.rs src-tauri/tests/lv1_actor.rs
git commit -m "feat: publish sequenced LV1 ping facts"
```

### Task 2: Add Pause-Safe Target Timing

**Files:**
- Modify: `src-tauri/src/fade/tick.rs`

**Interfaces:**
- Produces: `ActiveTarget::pause(now: Instant)`.
- Produces: `ActiveTarget::resume(now: Instant)`.
- Produces: `ActiveTarget::is_paused() -> bool`.
- Consumes: existing `started_at`, `value_at`, `is_done`, and `next_send` timing.

- [ ] **Step 1: Write pure failing timing tests**

Add tests proving that a 1-second target paused at 400 ms and resumed 2 seconds later is still at 400 ms progress, and that a repeated `pause` does not reset the original pause point:

```rust
#[test]
fn resume_rebases_started_at_by_the_full_pause_duration() {
    let mut target = make_channel(-20.0, -10.0, 1_000);
    let started_at = target.started_at;
    target.pause(started_at + Duration::from_millis(400));
    target.resume(started_at + Duration::from_millis(2_400));

    assert!((target.value_at(started_at + Duration::from_millis(2_400)) - -16.0).abs() < 1e-10);
    assert!(!target.is_done(started_at + Duration::from_millis(2_900)));
    assert!(target.is_done(started_at + Duration::from_millis(3_000)));
}

#[test]
fn repeated_pause_keeps_the_original_pause_boundary() {
    let mut target = make_channel(-20.0, -10.0, 1_000);
    let started_at = target.started_at;
    target.pause(started_at + Duration::from_millis(200));
    target.pause(started_at + Duration::from_millis(600));
    target.resume(started_at + Duration::from_millis(1_200));

    assert_eq!(target.started_at, started_at + Duration::from_millis(1_000));
}
```

- [ ] **Step 2: Run the pure tests and verify RED**

Run: `cargo nextest run -p advanced-show-control fade::tick::tests`

Expected: compilation fails because pause/resume methods are absent.

- [ ] **Step 3: Implement minimal pause/rebase state**

Add `paused_since: Option<Instant>` to `ActiveTarget`, initialize it to `None`, and implement:

```rust
pub(crate) fn pause(&mut self, now: Instant) {
    if self.paused_since.is_none() {
        self.paused_since = Some(now);
    }
}

pub(crate) fn resume(&mut self, now: Instant) {
    if let Some(paused_since) = self.paused_since.take() {
        self.started_at += now.duration_since(paused_since);
    }
}

pub(crate) fn is_paused(&self) -> bool {
    self.paused_since.is_some()
}
```

The actor will not call interpolation/send functions while paused; do not add hidden clock substitution inside `value_at`.

- [ ] **Step 4: Run the pure tests and verify GREEN**

Run: `cargo nextest run -p advanced-show-control fade::tick::tests`

Expected: all target timing, interpolation, override, and pause tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/fade/tick.rs
git commit -m "feat: pause and resume fade target timing"
```

### Task 3: Model the Generation-Wide Readiness Barrier

**Files:**
- Modify: `src-tauri/src/fade/state.rs`
- Test: `src-tauri/src/fade/state.rs`

**Interfaces:**
- Consumes: `Lv1StateSnapshot::ping_sequence` from Task 1.
- Consumes: `ActiveTarget::pause` and `resume` from Task 2.
- Produces: `READINESS_PINGS_REQUIRED: u8 = 2` and `READINESS_TIMEOUT: Duration = Duration::from_secs(5)`.
- Produces: barrier start/reset, ping observation, deadline, and cancellation methods used by Task 4.
- Produces: `EngineState::generation() -> u64` and `EngineState::readiness_timeout_context() -> Option<ReadinessTimeoutContext>` for actor checks and logging.

- [ ] **Step 1: Write failing pure barrier transition tests**

Cover a boundary of `10`, duplicate/stale sequence rejection, release on sequences `11` then `12`, reset to boundary `20`, same-generation enforcement, and pause preservation across reset. Assert outcomes through a small enum:

```rust
assert_eq!(state.observe_ping(4, 10, now), PingGateProgress::Ignored);
assert_eq!(state.observe_ping(4, 11, now), PingGateProgress::Waiting { observed: 1 });
assert_eq!(state.observe_ping(4, 11, now), PingGateProgress::Ignored);
assert_eq!(state.observe_ping(3, 12, now), PingGateProgress::Ignored);
assert_eq!(state.observe_ping(4, 12, now), PingGateProgress::Released);
```

- [ ] **Step 2: Run state tests and verify RED**

Run: `cargo nextest run -p advanced-show-control fade::state::tests`

Expected: compilation fails because readiness types and methods are absent.

- [ ] **Step 3: Add private barrier state**

Implement the following shape in `fade/state.rs`:

```rust
pub(crate) const READINESS_PINGS_REQUIRED: u8 = 2;
pub(crate) const READINESS_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct ReadinessBarrier {
    pub(crate) generation: u64,
    pub(crate) scene_index: i32,
    pub(crate) scene_name: String,
    pub(crate) last_counted_ping_sequence: u64,
    pub(crate) observed_ping_count: u8,
    pub(crate) deadline: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PingGateProgress {
    Ignored,
    Waiting { observed: u8 },
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadinessTimeoutContext {
    pub(crate) generation: u64,
    pub(crate) scene_index: i32,
    pub(crate) scene_name: String,
    pub(crate) observed_ping_count: u8,
}
```

Add `readiness_barrier: Option<ReadinessBarrier>` to `EngineState`. `start_or_reset_readiness` must pause every channel without changing an existing `paused_since`. `observe_ping` must require matching generation and a strictly increasing sequence. On release, clear the barrier and resume all channels at the supplied `Instant`.

- [ ] **Step 4: Make cancellation clear targets and barrier atomically**

Change `cancel_all_in_place` to clear both fields:

```rust
pub(crate) fn cancel_all_in_place(&mut self) {
    self.channels.clear();
    self.readiness_barrier = None;
}
```

Expose `generation()`, `readiness_deadline()`, `readiness_timeout_context()`, and `is_waiting_for_readiness()` for the actor loop without exposing mutable barrier internals.

- [ ] **Step 5: Run state and tick tests and verify GREEN**

Run: `cargo nextest run -p advanced-show-control 'fade::state::tests|fade::tick::tests'`

Expected: all barrier transition and target timing tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/fade/state.rs src-tauri/src/fade/tick.rs
git commit -m "feat: model post-recall ping readiness"
```

### Task 4: Gate Fade Writes Until Two Pings

**Files:**
- Modify: `src-tauri/src/fade/actor.rs:75-268`
- Modify: `src-tauri/src/fade/actor.rs:296-407`
- Test: `src-tauri/src/fade/actor.rs`

**Interfaces:**
- Consumes: `Lv1Event::PingReceived { sequence }` and snapshot `ping_sequence`.
- Consumes: `EngineState::{start_or_reset_readiness, observe_ping, readiness_deadline, is_waiting_for_readiness}`.
- Preserves: `FadeCommand::RecallSceneFade` public command shape.

- [ ] **Step 1: Write failing actor tests for initial blocking and release**

Using the existing fake LV1 mailbox harness, return a connected snapshot with `ping_sequence: 40`, send a timed recall, and assert no `WriteBatch` before these events:

```rust
event_bus.publish(AppEvent::Lv1 {
    generation: 7,
    event: Lv1Event::PingReceived { sequence: 41 },
});
// Still no WriteBatch.
event_bus.publish(AppEvent::Lv1 {
    generation: 7,
    event: Lv1Event::PingReceived { sequence: 42 },
});
// A later tick produces WriteBatch.
```

Add sibling cases proving sequence `40`, duplicate `41`, and generation `6` do not release.

- [ ] **Step 2: Run targeted tests and verify RED**

Run: `cargo nextest run -p advanced-show-control fade::actor::tests::post_recall_ping`

Expected: writes occur before two pings or tests fail because barrier methods are not wired.

- [ ] **Step 3: Establish the barrier only after successful timed recall setup**

In `handle_recall_scene_fade`, preserve the zero-duration return path. After all timed targets are inserted, call:

```rust
state.start_or_reset_readiness(
    expected_generation.unwrap_or(state.generation()),
    config.scene.index,
    config.scene.name,
    snapshot.ping_sequence,
    now,
);
```

Do not establish/reset the barrier before generation checks, fresh snapshot success, or target creation.

- [ ] **Step 4: Suppress the tick branch while waiting**

At the start of the tick branch, skip interpolation, completion, removal, and writes while `state.is_waiting_for_readiness()` is true. Keep the interval alive so release resumes normal scheduling.

- [ ] **Step 5: Count same-generation ping facts and release**

Add a specific event arm before the generic `Ok(_)` arm:

```rust
Ok(AppEvent::Lv1 {
    generation: event_generation,
    event: Lv1Event::PingReceived { sequence },
}) => {
    match state.observe_ping(event_generation, sequence, Instant::now()) {
        PingGateProgress::Ignored => {}
        PingGateProgress::Waiting { observed } => tracing::debug!(
            event = "fade_post_recall_ping_waiting",
            observed,
            required = READINESS_PINGS_REQUIRED,
            "Fade readiness is waiting for LV1 keepalive pings"
        ),
        PingGateProgress::Released => tracing::debug!(
            event = "fade_post_recall_ping_released",
            "Fade readiness released after LV1 keepalive resumed"
        ),
    }
}
```

- [ ] **Step 6: Add reset, overlap, and rebase actor tests**

Prove behavior through observable writes:

- Start Scene A, release it, advance partway, then recall Scene B and verify A sends no writes during B's gate.
- Send one ping, issue another valid timed recall, then prove two newer pings are required.
- Verify Scene B replaces only overlapping keys while Scene A's unrelated keys resume.
- Compare the first post-release value against expected pre-pause progress, not wall-clock progress.

- [ ] **Step 7: Run targeted fade tests and verify GREEN**

Run: `cargo nextest run -p advanced-show-control 'post_recall_ping|replacement_fade_starts|different_scene_fades'`

Expected: all readiness, overlap, and replacement tests pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/fade/actor.rs src-tauri/src/fade/state.rs
git commit -m "feat: gate fade writes on LV1 pings"
```

### Task 5: Abort Safely on Timeout and Lifecycle Changes

**Files:**
- Modify: `src-tauri/src/fade/actor.rs`
- Test: `src-tauri/src/fade/actor.rs`

**Interfaces:**
- Consumes: readiness deadline and barrier diagnostic fields from Task 3.
- Produces: stable warning event `fade_post_recall_ping_timeout`.
- Preserves: one `FadeEvent::FadeAborted` for timeout/disconnect cancellation.

- [ ] **Step 1: Write failing timeout and cancellation actor tests**

With paused Tokio time, establish a barrier, send fewer than two pings, advance five seconds, and assert:

```rust
assert!(matches!(fade_event, FadeEvent::FadeAborted));
assert_no_write_batch_after_timeout().await;
assert_eq!(captured_warn.event, "fade_post_recall_ping_timeout");
assert!(captured_warn.message.contains(
    "Fades were aborted because LV1 did not resume its keepalive cadence after scene recall"
));
```

Add separate cases for `AbortAll`, `Lv1Event::Disconnected`, and `RuntimeLifecycleEvent::ActiveGenerationChanged`, each proving later ping facts cannot produce writes. Keep tracing setup local until #38 is implemented.

- [ ] **Step 2: Run targeted tests and verify RED**

Run: `cargo nextest run -p advanced-show-control 'post_recall_ping_timeout|post_recall_ping_abort|post_recall_ping_disconnect|post_recall_ping_generation'`

Expected: timeout hangs/fails or cancellation leaves the barrier able to release.

- [ ] **Step 3: Add a dynamic deadline future to the actor loop**

Build a pending-or-deadline future beside `tick_fut`:

```rust
let readiness_deadline = state.readiness_deadline();
let readiness_timeout_fut = async move {
    match readiness_deadline {
        Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
        None => std::future::pending::<()>().await,
    }
};
```

On timeout, capture barrier diagnostics, call `cancel_all_in_place`, clear `tick_interval`, emit one `FadeAborted`, and log:

```rust
tracing::warn!(
    event = "fade_post_recall_ping_timeout",
    generation,
    scene_index,
    scene_name = %scene_name,
    observed_ping_count,
    timeout_ms = 5_000_u64,
    "Fades were aborted because LV1 did not resume its keepalive cadence after scene recall"
);
```

- [ ] **Step 4: Clear readiness on every lifecycle cancellation path**

- `AbortAll`: existing `cancel_all_in_place` now clears the barrier.
- `Disconnected`: cancel when either targets or a barrier exists, then emit one abort.
- `ActiveGenerationChanged`: if it differs from the engine generation, cancel immediately and emit no later success/write log.
- Actor shutdown: exit without writes; owned state drops.

Manual override remains target-scoped while waiting and must not release or reset the global barrier.

- [ ] **Step 5: Prove zero-duration and blocked recall behavior remain unchanged**

Run existing zero-duration fade tests and targeted scenes tests that prove blocked/skipped/disabled decisions do not send `FadeCommand::RecallSceneFade`:

Run: `cargo nextest run -p advanced-show-control 'zero_duration|blocked_recall|disabled|skipped'`

Expected: all selected tests pass without requiring ping facts for zero-duration writes.

- [ ] **Step 6: Run all fade and scenes tests and verify GREEN**

Run: `cargo nextest run -p advanced-show-control 'fade|scene_recall'`

Expected: all selected tests pass; no timeout test sleeps in real time.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/fade/actor.rs
git commit -m "fix: abort fades when LV1 readiness stalls"
```

### Task 6: Document and Verify the Runtime Contract

**Files:**
- Modify: `docs/architecture.md`
- Verify: workspace

**Interfaces:**
- Documents: `Lv1Event::PingReceived`, the two-ping barrier, global pause/rebase behavior, simulator limitation, and five-second abort.

- [ ] **Step 1: Update architecture documentation**

Add the ping fact to the LV1 event description and add a fade-readiness subsection stating:

```text
Every validated timed scene recall pauses all active fade writes for the current runtime generation. FadeEngine resumes and rebases those targets only after two later LV1 keepalive ping facts. A newer validated recall resets the count. Five seconds without readiness aborts all paused fades. This relies on an unconfirmed real-hardware assumption; the simulator does not exhibit ping delay during recall.
```

Reference #42 rather than documenting same-scene finishing as implemented by this change.

- [ ] **Step 2: Run formatting**

Run: `make fmt`

Expected: command exits successfully with no formatting differences remaining.

- [ ] **Step 3: Run targeted Rust verification**

Run: `cargo nextest run -p advanced-show-control 'lv1_actor|fade|scene_recall'`

Expected: all selected tests pass.

- [ ] **Step 4: Run broad verification**

Run: `make check`

Expected: Rust formatting, Clippy, Rust tests/build, frontend formatting, lint, typecheck, tests, Storybook tests, and build all pass.

- [ ] **Step 5: Inspect the final diff and safety invariants**

Run: `git status --short` and `git diff --check`.

Confirm from the diff that:

- only validated timed recalls establish the barrier;
- stale/pre-boundary pings cannot count;
- no tick/final writes occur while waiting;
- timeout/disconnect/generation/Abort All clear targets and barrier;
- zero-duration behavior is unchanged;
- normal pings do not create frontend log noise.

- [ ] **Step 6: Commit**

```bash
git add docs/architecture.md
git commit -m "docs: describe post-recall ping readiness"
```

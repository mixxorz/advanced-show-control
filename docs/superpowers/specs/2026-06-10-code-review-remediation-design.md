# Code Review Remediation Design

**Date:** 2026-06-10  
**Source:** CODE_REVIEW.md — five-phase recommended refactor plan

## Context

A full code review of the codebase identified 13 high-severity findings and ~15 medium ones. Two are data-integrity regressions introduced during the ShowState extraction refactor: show mutators silently swallow domain validation errors and never mark the show file dirty. An orphaned test file (`capture_tests.rs`) was supposed to guard exactly that behavior but was never re-declared in `mod.rs`. Additional issues include a reliability bug (idle connections never detect ping timeout), a safety-named test that asserts nothing, fader write failures that vanish silently, architectural dead code from the same refactor, and an unprotected TOCTOU window in the generation guard. The UI has a stale-snapshot race where five unversioned sources apply `AppViewState` last-write-wins.

This design follows the reviewer's own five-phase recommended refactor plan, reordering only to keep the two critical data-loss bugs first within Phase 1.

---

## Phase 1 — Immediate Safety/Correctness Fixes

### 1a. Restore `capture_tests.rs` (finding 3)

**File:** `src-tauri/src/app_state/mod.rs`

Add `#[cfg(test)] mod capture_tests;` to `mod.rs`. The file already exists at `src-tauri/src/app_state/capture_tests.rs` but references the pre-refactor API (`inner.scene_configs[0]`). Port all assertions to use the current `ShellState` / `AppViewState` API. The tests assert `snapshot.show_file_dirty` after each mutator call and `unwrap_err()` on invalid inputs — these are the regression guards for findings 1 and 2.

### 1b. Fix swallowed validation errors + dirty flag (findings 1, 2)

**File:** `src-tauri/src/app_state/shell.rs`, lines 93–181

All 6 show mutators use the pattern:
```rust
let _ = self.show.set_scene_duration(scene_id, duration_ms)
    .await
    .map_err(|err| format!("{err:?}"))?;
```

The `?` only propagates `ShowActorError` (the outer Result), which is never actually constructed — `ShowStateHandle` is mutex-backed and never errors that way. The inner `Result<bool, String>` carrying real validation errors is silently discarded, and `show_file_dirty` is never set.

**Fix for each of the 6 mutators:**
```rust
let changed = self.show.set_scene_duration(scene_id, duration_ms)
    .await
    .map_err(|err| format!("{err:?}"))?   // propagates outer (never fires)
    ?;                                     // propagates inner validation error
if changed {
    self.inner.lock().await.show_file_dirty = true;
}
```

Once the inner `?` is in place, the captured-tests will be able to assert both `unwrap_err()` on bad input and `show_file_dirty == true` on success.

### 1c. Fix idle ping timeout (finding 5)

**File:** `src/lv1/actor.rs`, connected loop (`run_connected`), around line 257

The current code checks `last_ping.elapsed() > PING_TIMEOUT` only at the top of the loop. If the peer goes silent and no commands arrive, the `select!` never wakes. 

**Fix:** Add a sleep branch inside the existing `tokio::select!`:
```rust
_ = tokio::time::sleep_until((state.last_ping + PING_TIMEOUT).into()) => {
    return DisconnectReason::PingTimeout;
}
```

**Test:** Add an integration test in `tests/lv1_actor.rs` that connects to a fake server that stops sending after the handshake and asserts that the actor publishes `Lv1Event::Disconnected` within roughly `PING_TIMEOUT`.

### 1d. Fix no-assertion generation test (finding 7)

**File:** `src/scene_recall/actor.rs`, test `stale_generation_does_not_start_fade` (lines 394–414)

The test publishes a `SceneChanged` and aborts immediately with no assertions. 

**Fix:** Install a fake fade handle on the command bus, advance paused time past the 25 ms settle delay + snapshot poll window, then assert:
- Zero `start_fade` calls were made
- No `SceneRecallEvent::StartRequested` was published

Also add a second test `generation_flip_between_decision_and_dispatch` that advances time to the moment between policy decision and `start_fade` call, flips the generation, and asserts no fade starts.

### 1e. Surface fade write failures (finding 8)

**File:** `src/fade/actor.rs`, function `send_batch` (line 241–243)

Currently:
```rust
async fn send_batch(command_bus: &AppCommandBus, writes: Vec<Lv1ParameterWrite>) {
    let _ = command_bus.write_batch(writes).await;
}
```

**Fix:** Log (rate-limited) and/or publish a `FadeEvent::WriteFailed` event when `write_batch` returns an error. The exact event variant may need to be added to `src/fade/events.rs`.

The `write_batch` in `src/runtime/commands.rs` already calls `publish_failure` — confirm this is routed somewhere visible (not just stderr) as part of this fix. Document in `docs/architecture.md` whether fire-and-forget for the 25 Hz stream is the deliberate trade-off.

---

## Phase 2 — High-Leverage Simplifications

### 2a. Delete `ShowActorError` double-Result (finding 4)

**File:** `src/show/handle.rs`

`ShowActorError` was an actor-era type that is never constructed: `ShowStateHandle` is mutex-backed and cannot fail with `CommandChannelClosed` or `ReplyChannelClosed`. All 6 methods currently return `Result<Result<bool, String>, ShowActorError>`.

**Fix:**
- Delete `ShowActorError` enum
- Change all 6 methods to return `Result<bool, String>` directly
- Update ~15 call sites in `shell.rs`, `events.rs`, `src-tauri/src/commands.rs` — remove the outer `map_err` and the double-`?` from Phase 1b (it becomes a single `?`)

This mechanically eliminates the structural precondition for findings 1 and 2 reoccurring.

### 2b. Delete dead duration-zero dedup (finding 12)

**Files:** `src/scene_recall/actor.rs`, `src/scene_recall/state.rs`

Three mechanisms all claim to own "duration is 0" dedup behavior but none does anything:
- `duration_zero_logged: HashSet<String>` in the actor
- The `reason != "duration is 0"` string-match gate in `Skipped` publication
- A parallel `duration_zero_logged` field in `SceneRecallState` plus `reset_for_generation()`

**Fix:** Delete all three. If reason-specific skip handling returns, make `RecallPolicyDecision::Skip` carry an enum reason variant instead of a free-form string.

### 2c. Delete dead `ShowEvent` machinery

**File:** `src/show/events.rs`

`ShowEvent::StateChanged`, `SceneConfigChanged`, `LockoutChanged` are declared and matched in `AppEvent` arms but never published in production.

**Fix:** Delete `ShowEvent` and all `AppEvent::Show(...)` match arms. If batched-writes work (planned in `docs/superpowers/plans/2026-06-10-lv1-batched-writes-writer-task.md`) intends to use them, wire them up or leave a `// TODO:` comment — don't leave dead variants active.

### 2d. Extract shared Lv1Command drain helper

**File:** `src/lv1/actor.rs`

There are 3 exhaustive `Lv1Command` match blocks (`drain_commands_for`, post-connect stale drain, connected loop). `Flush` replies differ unexplainedly between them.

**Fix:** Extract a `drain_pending_commands(cmd_rx, &writer_tx)` helper that handles all variants consistently; document why `Flush` differs across contexts if it must. Reference the three recent flush-fix commits to explain the invariant.

---

## Phase 3 — Enforce the Generation Invariant

### 3a. Generation-checked bus methods (finding 6)

**File:** `src/runtime/commands.rs`

`get_generation` and `set_generation` are separate lock acquisitions, leaving a TOCTOU window. The recall actor does `get_generation()` then `start_fade()` with an await in between.

**Fix:** Add `start_fade_if_generation(expected: u64, config: FadeConfig) -> Result<FadeHandle, StaleGeneration>` to `AppCommandBus` that checks the generation and clones the target under the same lock. Make `clear_targets()` bump the generation (currently it doesn't, so disconnect correctness depends on callers remembering to call both).

### 3b. Fix stale-event race in `events.rs` SceneListChanged (finding 9)

**File:** `src-tauri/src/app_state/events.rs`, lines 218–283

The code checks generation, drops the lock, calls `self.show.reconcile_scene_list(...)` (which mutates app-lifetime `ShowState`), and only then re-checks generation. A disconnect/reconnect between the check and reconcile lets a stale event prune scene configs.

**Fix:** Hold the inner lock across the `show.reconcile_scene_list(...)` call, or add `reconcile_if(expected_generation, ...)` seam. Document the lock-ordering convention on `ShellState`.

### 3c. Collapse 7 repeated generation guards in recall actor

**File:** `src/scene_recall/actor.rs`

The `process_scene_observation` function checks `get_generation() != generation` seven times.

**Fix:** Extract a `publish_if_current(generation, bus, event) -> bool` helper and a `guard(expected, bus) -> impl Future<Output=bool>` wrapper to collapse the repeated check-then-publish pattern.

---

## Phase 4 — Test Coverage

| Gap | Location | Test to add |
|-----|----------|-------------|
| Generation guard (finding 7) | `src/scene_recall/actor.rs` | `stale_generation_does_not_start_fade` — install fake handle, advance time, assert zero fades |
| Generation flip mid-dispatch | `src/scene_recall/actor.rs` | `generation_flip_between_decision_and_dispatch` |
| Disconnect aborts active fade | `tests/fade_engine.rs` | Connect, start fade, disconnect LV1, assert `FadeAborted` |
| Override-of-last-target terminal event (finding 10) | `tests/fade_engine.rs` | Start fade, override sole target, assert terminal event |
| Show-file round-trip | `src-tauri/src/app_state/show_file_mapping_tests.rs` | Build full `ShowFile` → serialize → deserialize → compare |
| Per-mutator validation + dirty tests | `src-tauri/src/app_state/capture_tests.rs` | All 6 mutators: error path (`unwrap_err`) + success+dirty path |
| Sleep-based test sync | `tests/runtime_bus.rs`, line 48–95 | Replace 200 ms sleep with channel/notification barrier |

---

## Phase 5 — Cleanup and Consistency

### 5a. Centralize `scene_id` format/parse

**Files:** `src/show/types.rs`, `src/show/capture.rs` (6 hand-rolled sites)

The `"{index}::{name}"` format is hand-rolled in 6 places across two crates, parsed back with silent fallbacks that produce junk `0::""` configs.

**Fix:** Add `SceneId::format(index, name) -> String` and `SceneId::parse(s) -> Result<(usize, String), ParseError>` to `src/show/types.rs`. Replace all 6 sites.

### 5b. Fix `MIN_SEND_DELTA_POS` unit bug

**File:** `src/fade/tick.rs`, lines 8–9, 120–128

The constant is documented as fader-position space (0.0–1.0) but is applied to pan (±100), balance, and width deltas, causing pan fades to send on effectively every tick.

**Fix:** Add per-parameter send deltas matching the existing override thresholds.

### 5c. Route `CommandFailed` and lag reports to in-app log

**Files:** `src-tauri/src/commands.rs` (line 675–781), `src/runtime/events.rs` (lines 43–45)

`CommandFailed` goes to stderr, invisible in a packaged Tauri app. Bus lag reporter also uses stderr.

**Fix:** Route both to `push_log` (the in-app diagnostic log already used elsewhere).

### 5d. Document recall timing windows

**File:** `src/scene_recall/actor.rs`

Four empirical timing windows (25 ms settle, 500 ms edit suppression, 2 s arming, 500 ms repeat delay) have no rationale in code or docs.

**Fix:** Add a comment block at the top of the actor loop explaining each window and why.

### 5e. UI `stateVersion` monotonic guard (finding 11)

**Files:** `src-tauri/src/app_state/view.rs`, `ui/src/App.tsx`

Five unordered async sources apply full `AppViewState` snapshots last-write-wins, causing intermittent stale-state overwrites.

**Fix:**
- Add `state_version: u64` to `AppViewState` in `view.rs`; increment it on every `snapshot_from_parts` call (use an `AtomicU64` or the existing generation value)
- In `App.tsx`, wrap `setAppState` in a guard: `if (next.state_version > current.state_version) setAppState(next)`

---

## Verification

After each phase, run:
```bash
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

For Phase 1 specifically:
- Re-enable `capture_tests` and confirm all 6 mutator tests pass
- Confirm `stale_generation_does_not_start_fade` fails before the fix and passes after

For Phase 5 UI change:
- Start the Tauri dev server, trigger a rapid reconnect sequence while a scene recall is in flight, and observe that the scene state does not snap back to a pre-recall version

---

## Open Questions (from CODE_REVIEW.md)

These are noted but not addressed in this plan:
1. Is `write_batch` fire-and-forget intentional for the 25 Hz stream? (Affects Phase 1e scope)
2. Is the dead `ShowEvent` machinery intended for planned batched-writes work?
3. Is retaining `scene_list` across disconnects intentional?
4. Is `SceneSummary.index` 0-based on real hardware? (Header/SceneTab off-by-one)
5. Should a recall blocked by the 2 s snapshot timeout be retried or surfaced more prominently?

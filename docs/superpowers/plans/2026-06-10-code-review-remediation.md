# Code Review Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Address all high-severity and key medium findings from CODE_REVIEW.md across five phases: correctness fixes, high-leverage simplifications, generation invariant enforcement, test coverage gaps, and cleanup + UI.

**Design doc:** `docs/superpowers/specs/2026-06-10-code-review-remediation-design.md`

**Tech Stack:** Rust, Tokio, Tauri (`src-tauri/`), React/TypeScript (`ui/`), nextest for Rust tests.

---

## Phase 1 — Immediate Safety/Correctness Fixes

### Task 1.1: Restore `capture_tests.rs` and fix show mutator regressions

**Files:**
- Modify: `src-tauri/src/app_state/mod.rs`
- Modify: `src-tauri/src/app_state/capture_tests.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Modify: `src/show/handle.rs` *(the double-Result wrapper, pre-phase 2)*

**Background:** `capture_tests.rs` was orphaned when `ShowState` was extracted into `ShowStateHandle`. The file exists but is not declared in `mod.rs`, so it never compiles. Its tests assert `snapshot.show_file_dirty == true` after mutator calls and `unwrap_err()` on invalid input. Both assertions fail against current code because:
1. Shell mutators use `let _ =` to discard the inner `Result<bool, String>` (validation errors silently no-op)
2. No mutator sets `show_file_dirty`

- [ ] **Step 1: Re-declare `capture_tests` in `mod.rs`**

  Add to `src-tauri/src/app_state/mod.rs`:
  ```rust
  #[cfg(test)]
  mod capture_tests;
  ```

- [ ] **Step 2: Run tests and observe failures**

  ```bash
  cargo nextest run -p advanced-show-control-tauri app_state::capture_tests
  ```
  Expected: FAIL (API mismatch and assertion failures — this confirms the regression).

- [ ] **Step 3: Port `capture_tests.rs` to the current API**

  The file references the pre-refactor `inner.scene_configs[0]` style. Update all test helpers to use the `ShellState` / `AppViewState` API that the rest of `events_tests.rs` uses. Do not change the assertions yet — they describe the desired behavior.

- [ ] **Step 4: Fix shell mutators to propagate validation errors and set dirty flag**

  In `src-tauri/src/app_state/shell.rs`, update all 6 mutators from:
  ```rust
  let _ = self.show.set_scene_duration(scene_id, duration_ms)
      .await
      .map_err(|err| format!("{err:?}"))?;
  Ok(self.snapshot().await)
  ```
  To:
  ```rust
  let changed = self.show.set_scene_duration(scene_id, duration_ms)
      .await
      .map_err(|err| format!("{err:?}"))?  // outer ShowActorError (never fires)
      ?;                                    // inner validation error
  if changed {
      self.inner.lock().await.show_file_dirty = true;
  }
  Ok(self.snapshot().await)
  ```

  Apply the same pattern to all 6 mutators:
  - `set_scene_duration_ms`
  - `store_scene_config`
  - `set_channel_scoped`
  - `set_all_channels_scoped`
  - `set_scene_scope_faders_enabled`
  - `set_scene_scope_pan_enabled`

- [ ] **Step 5: Run capture_tests and verify pass**

  ```bash
  cargo nextest run -p advanced-show-control-tauri app_state::capture_tests
  ```
  Expected: PASS.

- [ ] **Step 6: Run full test suite**

  ```bash
  cargo nextest run --workspace
  ```
  Expected: PASS.

- [ ] **Step 7: Commit**

  ```bash
  git add src-tauri/src/app_state/mod.rs src-tauri/src/app_state/capture_tests.rs src-tauri/src/app_state/shell.rs
  git commit -m "fix: restore capture_tests and propagate show mutator validation errors"
  ```

---

### Task 1.2: Fix idle ping timeout

**File:** `src/lv1/actor.rs`

**Background:** The connected loop checks `last_ping.elapsed() > PING_TIMEOUT` only at the top of the loop. If the peer goes silent and no commands or frames arrive, the `tokio::select!` never wakes and the actor stays `Connected` indefinitely.

- [ ] **Step 1: Add failing integration test**

  Add to `tests/lv1_actor.rs`:
  ```rust
  #[tokio::test]
  async fn silent_server_disconnects_after_ping_timeout() {
      // Server sends handshake + /Channels then goes silent
      // Assert actor publishes Lv1Event::Disconnected within ~PING_TIMEOUT
  }
  ```
  Use `tokio::time::pause()` and `tokio::time::advance()` to avoid a real 10-second wait.

- [ ] **Step 2: Run test and verify failure**

  ```bash
  cargo nextest run -p advanced-show-control --test lv1_actor silent_server_disconnects_after_ping_timeout
  ```
  Expected: FAIL (actor never disconnects).

- [ ] **Step 3: Add timer branch to connected loop select!**

  In `run_connected` in `src/lv1/actor.rs`, add inside the `tokio::select!` block:
  ```rust
  _ = tokio::time::sleep_until((state.last_ping + PING_TIMEOUT).into()) => {
      return DisconnectReason::PingTimeout;
  }
  ```

- [ ] **Step 4: Run test and verify pass**

  ```bash
  cargo nextest run -p advanced-show-control --test lv1_actor silent_server_disconnects_after_ping_timeout
  ```
  Expected: PASS.

- [ ] **Step 5: Run full test suite**

  ```bash
  cargo nextest run --workspace
  ```
  Expected: PASS.

- [ ] **Step 6: Commit**

  ```bash
  git add src/lv1/actor.rs tests/lv1_actor.rs
  git commit -m "fix: detect ping timeout on idle connections"
  ```

---

### Task 1.3: Fix no-assertion generation test

**File:** `src/scene_recall/actor.rs`

**Background:** `stale_generation_does_not_start_fade` publishes a `SceneChanged` and aborts immediately. No fake fade handle is installed and no assertions are made. It passes regardless of behavior.

- [ ] **Step 1: Rewrite the test with assertions**

  Update `stale_generation_does_not_start_fade` to:
  - Install a fake fade handle that records `start_fade` calls
  - Advance paused time past the 25 ms settle delay + snapshot poll window
  - Assert zero `start_fade` calls
  - Assert no `SceneRecallEvent::StartRequested` was published

- [ ] **Step 2: Add TOCTOU generation flip test**

  Add `generation_flip_between_decision_and_dispatch`:
  - Set up with generation 1, arm recall state
  - Advance to just after settle (policy decision made)
  - Flip generation to 2 (simulating reconnect)
  - Assert no fade starts despite policy returning `Start`

- [ ] **Step 3: Run tests**

  ```bash
  cargo nextest run -p advanced-show-control scene_recall::actor::tests
  ```
  Expected: PASS.

- [ ] **Step 4: Commit**

  ```bash
  git add src/scene_recall/actor.rs
  git commit -m "fix: add assertions to generation guard test"
  ```

---

### Task 1.4: Surface fade write failures

**Files:** `src/fade/actor.rs`, `src/fade/events.rs`

**Background:** `send_batch` silently discards all write errors with `let _ = ...`. A fade can appear to run while doing nothing.

- [ ] **Step 1: Add `WriteFailed` event variant if not present**

  In `src/fade/events.rs`, add (if not already present):
  ```rust
  WriteFailed { reason: String },
  ```

- [ ] **Step 2: Update `send_batch` to surface errors**

  Replace:
  ```rust
  async fn send_batch(command_bus: &AppCommandBus, writes: Vec<Lv1ParameterWrite>) {
      let _ = command_bus.write_batch(writes).await;
  }
  ```
  With:
  ```rust
  async fn send_batch(
      command_bus: &AppCommandBus,
      event_bus: &AppEventBus,
      writes: Vec<Lv1ParameterWrite>,
  ) {
      if let Err(err) = command_bus.write_batch(writes).await {
          event_bus.publish(AppEvent::Fade(FadeEvent::WriteFailed {
              reason: format!("{err:?}"),
          }));
      }
  }
  ```
  Update all call sites to pass `event_bus`.

- [ ] **Step 3: Run tests**

  ```bash
  cargo nextest run --workspace
  ```
  Expected: PASS.

- [ ] **Step 4: Commit**

  ```bash
  git add src/fade/actor.rs src/fade/events.rs
  git commit -m "fix: surface fade write failures as events"
  ```

---

## Phase 2 — High-Leverage Simplifications

### Task 2.1: Delete `ShowActorError` double-Result

**Files:** `src/show/handle.rs`, `src-tauri/src/app_state/shell.rs`, `src-tauri/src/app_state/events.rs`, `src-tauri/src/commands.rs`

**Background:** `ShowActorError` is a dead type from the actor era. `ShowStateHandle` is mutex-backed and its variants are never constructed. The nested `Result<Result<bool, String>, ShowActorError>` shape forces awkward double-`?` at every call site and obscures real validation errors.

- [ ] **Step 1: Simplify `ShowStateHandle` return types**

  In `src/show/handle.rs`:
  - Delete `ShowActorError` enum
  - Change all 6 methods returning `Result<Result<bool, String>, ShowActorError>` to return `Result<bool, String>` directly
  - Methods returning `Result<T, ShowActorError>` for other types: simplify to return `T` directly (e.g., `get_snapshot` → `ShowSnapshot`, `get_lockout` → `bool`)

- [ ] **Step 2: Update all call sites**

  Remove the outer `map_err(|err| format!("{err:?}"))` and simplify double-`?` to single `?` in:
  - `src-tauri/src/app_state/shell.rs` (all 6 mutators from Task 1.1)
  - `src-tauri/src/app_state/events.rs` (reconcile and other callers)
  - `src-tauri/src/commands.rs` (any direct show handle calls)

- [ ] **Step 3: Run full test suite**

  ```bash
  cargo nextest run --workspace
  ```
  Expected: PASS.

- [ ] **Step 4: Commit**

  ```bash
  git add src/show/handle.rs src-tauri/src/app_state/shell.rs src-tauri/src/app_state/events.rs src-tauri/src/commands.rs
  git commit -m "refactor: delete ShowActorError and simplify show handle return types"
  ```

---

### Task 2.2: Delete dead duration-zero dedup

**Files:** `src/scene_recall/actor.rs`, `src/scene_recall/state.rs`

**Background:** Three mechanisms claim to own "duration is 0" dedup but none does anything. `decide_scene_recall` no longer produces that reason string; zero-duration scenes now `Start` directly.

- [ ] **Step 1: Delete from `actor.rs`**

  Remove:
  - `duration_zero_logged: HashSet<String>` field
  - The `if reason != "duration is 0" || duration_zero_logged.insert(scene_id)` gate in `Skipped` publication
  - Any import of `HashSet` that becomes unused

- [ ] **Step 2: Delete from `state.rs`**

  Remove:
  - The `duration_zero_logged` field from `SceneRecallState`
  - `reset_for_generation()` method

- [ ] **Step 3: Run tests**

  ```bash
  cargo nextest run -p advanced-show-control scene_recall
  ```
  Expected: PASS.

- [ ] **Step 4: Commit**

  ```bash
  git add src/scene_recall/actor.rs src/scene_recall/state.rs
  git commit -m "refactor: delete dead duration-zero dedup code"
  ```

---

### Task 2.3: Delete dead `ShowEvent` machinery

**File:** `src/show/events.rs`, `src/runtime/events.rs`

**Background:** `ShowEvent::StateChanged`, `SceneConfigChanged`, `LockoutChanged` are declared and matched but never published. They mislead about where dirty-tracking lives.

- [ ] **Step 1: Delete `ShowEvent` enum and all match arms**

  - Delete `src/show/events.rs` contents (or the file if it becomes empty)
  - Remove `AppEvent::Show(ShowEvent)` variant from `src/runtime/events.rs`
  - Remove all `AppEvent::Show(_) => {}` match arms throughout the codebase

- [ ] **Step 2: Run tests**

  ```bash
  cargo nextest run --workspace
  ```
  Expected: PASS.

- [ ] **Step 3: Commit**

  ```bash
  git add src/show/events.rs src/runtime/events.rs
  git commit -m "refactor: delete dead ShowEvent machinery"
  ```

---

### Task 2.4: Extract shared Lv1Command drain helper

**File:** `src/lv1/actor.rs`

**Background:** Three exhaustive `Lv1Command` match blocks exist with subtle behavioral differences (especially `Flush`). This is hard to reason about and has already caused bugs (3 flush-fix commits).

- [ ] **Step 1: Extract helper**

  Create `fn drain_command(cmd: Lv1Command, is_connected: bool)` or `fn handle_disconnected_command(cmd: Lv1Command)` that covers the shared arms. Document why `Flush` returns `Ok(())` in one context and `Err(NotConnected)` in another.

- [ ] **Step 2: Replace duplicated match blocks with calls to helper**

- [ ] **Step 3: Run tests**

  ```bash
  cargo nextest run -p advanced-show-control
  cargo nextest run -p advanced-show-control --test lv1_actor
  ```
  Expected: PASS.

- [ ] **Step 4: Commit**

  ```bash
  git add src/lv1/actor.rs
  git commit -m "refactor: extract shared lv1 command drain helper"
  ```

---

## Phase 3 — Enforce the Generation Invariant

### Task 3.1: Generation-checked bus method and `clear_targets` bump

**File:** `src/runtime/commands.rs`

**Background:** `get_generation()` and `set_generation()` are separate lock acquisitions. The recall actor calls `get_generation()` then awaits `start_fade()` — a window where a new LV1 target could be installed. `clear_targets()` also doesn't bump the generation, so disconnect correctness depends on callers doing both.

- [ ] **Step 1: Add `start_fade_if_generation`**

  Add to `AppCommandBus`:
  ```rust
  pub async fn start_fade_if_generation(
      &self,
      expected: u64,
      config: FadeConfig,
  ) -> Result<FadeHandle, StaleGeneration> {
      let mut targets = self.targets.lock().await;
      if targets.generation != expected {
          return Err(StaleGeneration);
      }
      let engine = targets.fade_engine.clone().ok_or(StaleGeneration)?;
      drop(targets);
      engine.start_fade(config).await.map_err(|_| StaleGeneration)
  }
  ```

- [ ] **Step 2: Make `clear_targets` bump generation**

  In `clear_targets()`, increment `targets.generation` so any in-flight recall tasks see a stale generation automatically.

- [ ] **Step 3: Update recall actor to use `start_fade_if_generation`**

  Replace the `get_generation() != generation → return` guard + `start_fade()` pattern in `process_scene_observation` with a single call to `start_fade_if_generation(generation, fade_config)`.

- [ ] **Step 4: Run tests**

  ```bash
  cargo nextest run --workspace
  ```
  Expected: PASS.

- [ ] **Step 5: Commit**

  ```bash
  git add src/runtime/commands.rs src/scene_recall/actor.rs
  git commit -m "fix: enforce generation guard atomically in recall path"
  ```

---

### Task 3.2: Fix stale-event race in `SceneListChanged`

**File:** `src-tauri/src/app_state/events.rs`, lines 218–283

**Background:** Generation is checked, lock dropped, `show.reconcile_scene_list(...)` mutates app-lifetime `ShowState`, then generation is re-checked. A disconnect/reconnect between check and reconcile can prune scene configs against a stale scene list.

- [ ] **Step 1: Hold inner lock across reconcile call**

  Restructure the `SceneListChanged` handler to either:
  - Hold `inner` lock across the `show.reconcile_scene_list(...)` call (following the existing `inner → show` lock ordering from `snapshot()`)
  - Or add `reconcile_if(expected_generation, ...) -> bool` seam to `ShowStateHandle` that checks and mutates atomically

- [ ] **Step 2: Add a doc comment on `ShellState` documenting lock ordering**

  ```rust
  // Lock ordering: always acquire `inner` before `show.state` to avoid deadlocks.
  ```

- [ ] **Step 3: Run tests**

  ```bash
  cargo nextest run -p advanced-show-control-tauri
  ```
  Expected: PASS.

- [ ] **Step 4: Commit**

  ```bash
  git add src-tauri/src/app_state/events.rs
  git commit -m "fix: hold lock across scene list reconciliation to prevent stale mutation"
  ```

---

### Task 3.3: Collapse repeated generation guards in recall actor

**File:** `src/scene_recall/actor.rs`

**Background:** `process_scene_observation` has 7 `get_generation() != generation → return` guards. With Task 3.1 done, several can be removed (the final `start_fade` guard is replaced by `start_fade_if_generation`). The remaining checks can use a helper.

- [ ] **Step 1: Remove guards made redundant by Task 3.1**

  The guard immediately before `start_fade()` can be deleted (now handled atomically).

- [ ] **Step 2: Extract `is_generation_current` async helper**

  ```rust
  async fn is_generation_current(expected: u64, command_bus: &AppCommandBus) -> bool {
      command_bus.get_generation().await == expected
  }
  ```
  Replace remaining inline checks.

- [ ] **Step 3: Run tests**

  ```bash
  cargo nextest run -p advanced-show-control scene_recall
  ```
  Expected: PASS.

- [ ] **Step 4: Commit**

  ```bash
  git add src/scene_recall/actor.rs
  git commit -m "refactor: collapse repeated generation guards in recall actor"
  ```

---

## Phase 4 — Test Coverage

### Task 4.1: Disconnect aborts active fade

**File:** `tests/fade_engine.rs`

- [ ] **Step 1: Add test**

  ```rust
  #[tokio::test]
  async fn disconnect_aborts_active_fade() {
      // Connect LV1, start a fade with duration > 0
      // Disconnect the fake LV1 server
      // Assert FadeAborted event is published
      // Assert no further FadeChannelCompleted events
  }
  ```

- [ ] **Step 2: Run test**

  ```bash
  cargo nextest run -p advanced-show-control --test fade_engine disconnect_aborts_active_fade
  ```
  Expected: PASS.

- [ ] **Step 3: Commit**

  ```bash
  git add tests/fade_engine.rs
  git commit -m "test: disconnect aborts active fade"
  ```

---

### Task 4.2: Override of last target emits terminal event (finding 10)

**File:** `tests/fade_engine.rs`

**Background:** `FadeCompleted` fires when targets drain via ticks, `FadeAborted` via abort/disconnect, but overriding the last target clears the tick interval with no terminal event.

- [ ] **Step 1: Add test**

  ```rust
  #[tokio::test]
  async fn override_of_last_target_emits_terminal_event() {
      // Start a fade with one target
      // Override that target (via manual LV1 value change past override threshold)
      // Assert FadeCompleted or FadeAborted is published
  }
  ```

- [ ] **Step 2: Fix the production code if the test fails**

  In `src/fade/actor.rs` override paths (lines 170–181, 245–273): after removing a target, if `!state.is_active()`, publish a terminal event.

- [ ] **Step 3: Run tests**

  ```bash
  cargo nextest run -p advanced-show-control --test fade_engine
  ```
  Expected: PASS.

- [ ] **Step 4: Commit**

  ```bash
  git add src/fade/actor.rs tests/fade_engine.rs
  git commit -m "fix: emit terminal event when last fade target is overridden"
  ```

---

### Task 4.3: Show-file structural round-trip test

**File:** `src-tauri/src/app_state/show_file_mapping_tests.rs`

**Background:** Show-file field additions are 5–6-file shotgun edits. A missed field compiles fine and silently drops data.

- [ ] **Step 1: Add round-trip test**

  Build a fully-populated `ShowFile` with all fields non-default → serialize to JSON → deserialize → compare field-by-field. The test documents which fields exist and catches any future missed mapping.

- [ ] **Step 2: Run test**

  ```bash
  cargo nextest run -p advanced-show-control-tauri app_state::show_file_mapping_tests
  ```
  Expected: PASS.

- [ ] **Step 3: Commit**

  ```bash
  git add src-tauri/src/app_state/show_file_mapping_tests.rs
  git commit -m "test: add show-file structural round-trip test"
  ```

---

### Task 4.4: Replace sleep-based test sync

**File:** `tests/runtime_bus.rs`, lines 48–95

**Background:** The cross-module bus test synchronizes with a 200 ms `sleep`. This is slow and flaky under load.

- [ ] **Step 1: Replace `sleep` with a barrier**

  Use a `tokio::sync::Notify` or `oneshot` channel to signal when the expected event has been processed.

- [ ] **Step 2: Run tests**

  ```bash
  cargo nextest run -p advanced-show-control --test runtime_bus
  ```
  Expected: PASS.

- [ ] **Step 3: Commit**

  ```bash
  git add tests/runtime_bus.rs
  git commit -m "test: replace sleep-based sync in runtime_bus tests"
  ```

---

## Phase 5 — Cleanup and Consistency

### Task 5.1: Centralize `scene_id` format/parse

**Files:** `src/show/types.rs`, `src/show/capture.rs` (and other sites)

- [ ] **Step 1: Add `SceneId` helpers in `types.rs`**

  ```rust
  pub fn format_scene_id(index: usize, name: &str) -> String {
      format!("{index}::{name}")
  }

  pub fn parse_scene_id(s: &str) -> Result<(usize, String), String> {
      // parse "index::name" with error on malformed input
  }
  ```

- [ ] **Step 2: Replace all 6 hand-rolled sites** with calls to the new helpers.

- [ ] **Step 3: Run tests**

  ```bash
  cargo nextest run --workspace
  ```
  Expected: PASS.

- [ ] **Step 4: Commit**

  ```bash
  git add src/show/types.rs src/show/capture.rs
  git commit -m "refactor: centralize scene_id format and parse"
  ```

---

### Task 5.2: Fix `MIN_SEND_DELTA_POS` unit bug

**File:** `src/fade/tick.rs`, lines 8–9, 120–128

**Background:** `MIN_SEND_DELTA_POS` is documented as fader-position space (0.0–1.0) but is applied to pan/balance/width deltas (±100), so pan fades send on every tick.

- [ ] **Step 1: Add per-parameter send deltas**

  Add constants matching the existing per-parameter override thresholds:
  ```rust
  const MIN_SEND_DELTA_POS: f64 = 0.001;   // fader position 0.0–1.0
  const MIN_SEND_DELTA_PAN: f64 = 0.5;     // pan ±100
  const MIN_SEND_DELTA_BALANCE: f64 = 0.5;
  const MIN_SEND_DELTA_WIDTH: f64 = 0.5;
  ```

  Apply the correct constant in `next_send` based on `self.key.parameter`.

- [ ] **Step 2: Run fade tests**

  ```bash
  cargo nextest run -p advanced-show-control fade
  cargo nextest run -p advanced-show-control --test fade_engine
  ```
  Expected: PASS.

- [ ] **Step 3: Commit**

  ```bash
  git add src/fade/tick.rs
  git commit -m "fix: use per-parameter send deltas in fade tick"
  ```

---

### Task 5.3: Route `CommandFailed` and lag reports to in-app log

**Files:** `src-tauri/src/commands.rs`, `src/runtime/events.rs`

- [ ] **Step 1: Route `CommandFailed` to `push_log`**

  In `src-tauri/src/commands.rs` projector (lines 675–781), replace `eprintln!` / stderr writes for `CommandFailed` with calls to `push_log`.

- [ ] **Step 2: Route bus lag reporter to `push_log`**

  In `src/runtime/events.rs` (lines 43–45), replace the `eprintln!` lag report with a `push_log` call.

- [ ] **Step 3: Run tests**

  ```bash
  cargo nextest run --workspace
  ```
  Expected: PASS.

- [ ] **Step 4: Commit**

  ```bash
  git add src-tauri/src/commands.rs src/runtime/events.rs
  git commit -m "fix: route CommandFailed and lag reports to in-app log"
  ```

---

### Task 5.4: Document recall timing windows

**File:** `src/scene_recall/actor.rs`

- [ ] **Step 1: Add comment block at top of actor loop**

  Explain the four empirical timing constants:
  - 25 ms settle — allows LV1 scene-state to stabilize after a scene change event
  - 500 ms edit suppression — suppresses recall when the scene list was recently edited
  - 2 s arming delay — the first scene seen after arming is treated as baseline, not a recall
  - 500 ms repeat delay — prevents the same scene from triggering two consecutive recalls

- [ ] **Step 2: Commit**

  ```bash
  git add src/scene_recall/actor.rs
  git commit -m "docs: document scene recall timing windows"
  ```

---

### Task 5.5: UI `stateVersion` monotonic guard (finding 11)

**Files:** `src-tauri/src/app_state/view.rs`, `ui/src/App.tsx`, `ui/src/commands.ts`

**Background:** Five async sources call `setAppState` with full snapshots in last-write-wins order. A slow `refreshLv1Discovery` can overwrite a more recent scene-recall push.

- [ ] **Step 1: Add `state_version` to `AppViewState`**

  In `src-tauri/src/app_state/view.rs`, add:
  ```rust
  pub state_version: u64,
  ```
  In `snapshot_from_parts`, increment a monotonic counter (use `AtomicU64` in `ShellInner` or derive from the existing generation value combined with a counter).

- [ ] **Step 2: Update TypeScript type**

  In `ui/src/types.ts` (or wherever `AppViewState` is defined), add:
  ```typescript
  state_version: number;
  ```

- [ ] **Step 3: Add versioned `setAppState` wrapper in `App.tsx`**

  Replace direct `setAppState(snapshot)` calls:
  ```typescript
  const applySnapshot = useCallback((next: AppViewState) => {
    setAppState(prev =>
      !prev || next.state_version > prev.state_version ? next : prev
    );
  }, []);
  ```
  Use `applySnapshot` in all 5 call sites.

- [ ] **Step 4: Build and manual test**

  ```bash
  cd ui && npm run build
  ```
  Start the Tauri dev server, trigger a rapid reconnect while a scene recall is in flight, and verify the scene state does not snap back to a pre-recall version.

- [ ] **Step 5: Commit**

  ```bash
  git add src-tauri/src/app_state/view.rs ui/src/App.tsx ui/src/types.ts
  git commit -m "fix: add stateVersion guard to prevent stale snapshot overwrites"
  ```

---

## Final Verification

- [ ] **Run full test suite**

  ```bash
  cargo nextest run --workspace
  ```
  Expected: PASS.

- [ ] **Format and lint**

  ```bash
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  ```
  Expected: both PASS.

- [ ] **Build**

  ```bash
  cargo build --workspace
  cd ui && npm run build
  ```
  Expected: both PASS.

- [ ] **Inspect git log**

  ```bash
  git log --oneline -20
  ```
  Expected: shows one commit per task from this plan.

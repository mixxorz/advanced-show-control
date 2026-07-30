# Atomic Disconnect Transaction Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace cancellation-prone disconnect claims with an atomic generation transaction and make reconnect timeout/completion arbitration exact and cancellation-safe.

**Architecture:** `RuntimeGeneration::advance_if_current` is the atomic cleanup claim. `AppLifecycle` advances generation and clears runtime-owned state without a later await, while the Show actor serializes exact reconnect timeout and completion attempts. Timeout cleanup runs in a detached Tokio task after a successful Show claim.

**Tech Stack:** Rust, Tokio actors/mutexes/tasks, Tauri command adapters, `AppEventBus`, `tracing`, cargo-nextest.

## Global Constraints

- Production runtime generation mutation is monotonic; arbitrary assignment is test-only.
- Generation advance, handle abort, runtime ownership clear, connection-state clear, and peer clear form one cancellation-safe lifecycle transaction.
- No manual disconnect claim survives across an await.
- Same-generation disconnect requests produce one changed result and one success fact/log sequence.
- Stale requests never block, abort, or clear newer generations.
- Reconnect timeout and reconnect completion are arbitrated by exact Show-owned attempt identity.
- Completion-first rejects timeout; timeout-first rejects later completion for that attempt.
- A successful timeout claim always launches detached generation-scoped cleanup before the caller can be cancelled.
- Normal disconnect publishes the old-generation disconnected fact before the new active-generation fact.
- Stale/no-op paths publish no success fact or `INFO` log.
- Tauri and React adapters contain no lifecycle or reconnect policy.
- Preserve lockout, exact scene identity, generation guards, fade abort, manual override, and LV1 write safety.
- Use pure unit tests for generation/state policy and lifecycle/actor tests with mailboxes, `AppEventBus`, tracing capture, and explicit gates; no sleeps or private actor-state mutation.
- Do not address settings persistence (#54) or cue-list reconnect identity (#66).

---

## File Map

- Modify `src-tauri/src/runtime/generation.rs`: add atomic conditional advancement.
- Modify `src-tauri/src/lifecycle/mod.rs`: replace manual claims with one atomic runtime-clear transaction, detach timeout cleanup, carry reconnect attempt identity through completion, and replace claim-era tests.
- Modify `src-tauri/src/show/commands.rs`: define explicit reconnect completion mode/outcome and exact timeout claim command replies.
- Modify `src-tauri/src/show/state.rs`: arbitrate timeout versus completion by attempt identity.
- Modify `src-tauri/src/show/actor.rs`: route completion/timeout outcomes through the Show mailbox.
- Modify `src-tauri/src/show/mod.rs`: re-export only the narrow internal types lifecycle needs.
- Modify `src-tauri/src/ui/commands/lifecycle.rs` only if needed to preserve the already-thin `attempt` forwarding adapter.
- Test in the inline unit/actor test modules for these files.

### Task 1: Replace Manual Claims With Atomic Cleanup and Attempt Arbitration

**Files:**
- Modify: `src-tauri/src/runtime/generation.rs`
- Modify: `src-tauri/src/lifecycle/mod.rs`
- Modify: `src-tauri/src/show/commands.rs`
- Modify: `src-tauri/src/show/state.rs`
- Modify: `src-tauri/src/show/actor.rs`
- Modify: `src-tauri/src/show/mod.rs`
- Possibly modify: `src-tauri/src/ui/commands/lifecycle.rs`

**Test styles:** Pure unit tests for `RuntimeGeneration` and `ShowState`; lifecycle/actor tests for runtime transaction, timeout cancellation, actor routing, facts, and logs.

**Interfaces:**
- Produces `RuntimeGeneration::advance_if_current(&self, expected: u64) -> Option<u64>`.
- Produces private `AppLifecycle::clear_runtime_if_current(expected_generation) -> Option<RuntimeClearTransaction>`.
- Produces Show-owned reconnect completion mode and explicit accepted/changed outcome.
- Keeps public `disconnect_current_runtime()` and `reconnect_timed_out(attempt)` signatures.

- [ ] **Step 1: Add failing pure unit tests for conditional generation advancement**

Add `runtime::generation` tests that express the interface before implementing it:

```rust
#[tokio::test]
async fn advance_if_current_advances_only_the_matching_generation() {
    let generation = RuntimeGeneration::new();
    let first = generation.advance().await;

    assert_eq!(generation.advance_if_current(first).await, Some(first + 1));
    assert_eq!(generation.advance_if_current(first).await, None);
    assert_eq!(generation.current().await, first + 1);
}
```

Run:

```bash
cargo nextest run -p advanced-show-control advance_if_current_advances_only_the_matching_generation --no-capture
```

Expected: compilation fails because `advance_if_current` does not exist.

- [ ] **Step 2: Implement atomic conditional advancement**

Add this production method without adding any production setter:

```rust
pub(crate) async fn advance_if_current(&self, expected: u64) -> Option<u64> {
    let mut current = self.current.lock().await;
    if *current != expected {
        return None;
    }
    *current = current.saturating_add(1);
    Some(*current)
}
```

Run the focused test and expect PASS.

- [ ] **Step 3: Add failing Show state tests for timeout/completion ordering**

Introduce the intended internal policy types in tests:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionCompletionMode {
    Unconditional,
    Reconnect { attempt: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompleteConnectionOutcome {
    pub accepted: bool,
    pub changed: bool,
}
```

Add this test helper beside the existing `identity` helper:

```rust
fn reconnecting_state(attempt: u64) -> ShowState {
    ShowState {
        connected_lv1_identity: Some(identity("old")),
        pending_lv1_identity: Some(identity("new")),
        reconnect: ReconnectState {
            active: true,
            attempt,
        },
        ..Default::default()
    }
}
```

Add pure tests proving:

```rust
#[test]
fn timeout_first_rejects_completion_for_the_same_attempt() {
    let mut state = reconnecting_state(4);
    assert!(state.claim_reconnect_timeout(4));

    let outcome = state.complete_lv1_connection(
        identity(),
        ConnectionCompletionMode::Reconnect { attempt: 4 },
    );

    assert_eq!(outcome, CompleteConnectionOutcome { accepted: false, changed: false });
}

#[test]
fn completion_first_rejects_timeout_for_the_same_attempt() {
    let mut state = reconnecting_state(4);
    let outcome = state.complete_lv1_connection(
        identity(),
        ConnectionCompletionMode::Reconnect { attempt: 4 },
    );

    assert!(outcome.accepted);
    assert!(!state.claim_reconnect_timeout(4));
}

#[test]
fn unconditional_completion_does_not_require_a_reconnect_attempt() {
    let mut state = ShowState::default();
    let outcome = state.complete_lv1_connection(
        identity(),
        ConnectionCompletionMode::Unconditional,
    );

    assert!(outcome.accepted);
}
```

Adapt existing Show state tests to the explicit outcome rather than overloading `changed: false`.

Run the three focused filters and confirm they fail before implementation.

- [ ] **Step 4: Implement Show-owned attempt arbitration**

Add private timeout memory to `ShowState`:

```rust
timed_out_reconnect_attempt: Option<u64>,
```

Implement completion policy so reconnect completion is accepted only for the currently active exact attempt and no timeout claim:

```rust
pub(crate) fn complete_lv1_connection(
    &mut self,
    identity: Lv1SystemIdentity,
    mode: ConnectionCompletionMode,
) -> CompleteConnectionOutcome {
    let accepted = match mode {
        ConnectionCompletionMode::Unconditional => true,
        ConnectionCompletionMode::Reconnect { attempt } => {
            self.reconnect.active
                && self.reconnect.attempt == attempt
                && self.timed_out_reconnect_attempt != Some(attempt)
        }
    };
    if !accepted {
        return CompleteConnectionOutcome {
            accepted: false,
            changed: false,
        };
    }

    let reconnect = ReconnectState::default();
    let changed = self.connected_lv1_identity.as_ref() != Some(&identity)
        || self.pending_lv1_identity.is_some()
        || self.reconnect != reconnect
        || self.timed_out_reconnect_attempt.is_some();
    self.connected_lv1_identity = Some(identity);
    self.pending_lv1_identity = None;
    self.reconnect = reconnect;
    self.timed_out_reconnect_attempt = None;
    CompleteConnectionOutcome {
        accepted: true,
        changed,
    }
}
```

`claim_reconnect_timeout` must require active exact identity, set `active = false`, and record the attempt. Clear timeout memory during reset, failure, runtime-disconnected, and accepted unconditional completion paths.

Update `ShowCommand::CompleteLv1Connection` to carry `mode` and reply with `CompleteConnectionOutcome`. Keep timeout claim handling inside the actor. Re-export only the internal policy/outcome types lifecycle uses.

Run:

```bash
cargo nextest run -p advanced-show-control show::state
cargo nextest run -p advanced-show-control show::actor
```

Expected: PASS.

- [ ] **Step 5: Add failing lifecycle tests for atomic cleanup**

Before replacing the manual claim, add deterministic tests for:

1. `two_same_generation_disconnects_have_one_success_sequence`
   - pause both public requests after generation capture;
   - release both;
   - assert exactly one `changed: true`, one disconnected fact, one active-generation fact, and one `lv1_disconnected` log.
2. `stale_generation_does_not_block_newer_disconnect`
   - capture `N`, advance/install `N+1`, then run cleanup for both;
   - assert `N` is unchanged and `N+1` disconnect succeeds.
3. `disconnect_public_path_installed_newer_runtime_is_unchanged`
   - gate public disconnect before transaction;
   - install newer runtime;
   - assert no old success fact/log and newer mailboxes remain usable.

Run each focused filter. Expected: at least the cross-generation test fails against `disconnecting_generation`, demonstrating the stale claim problem.

- [ ] **Step 6: Replace lifecycle claim/release with one atomic transaction**

Remove `disconnecting_generation`, `claim_disconnect_generation`, `release_disconnect_generation`, and `abort_runtime_handles_without_advancing_generation`.

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeClearTransaction {
    cleared_generation: u64,
    active_generation: u64,
}

async fn clear_runtime_if_current(
    &self,
    expected_generation: u64,
) -> Option<RuntimeClearTransaction> {
    let mut inner = self.inner.lock().await;
    let active_generation = inner
        .generation
        .advance_if_current(expected_generation)
        .await?;

    inner.handles.abort_all();
    inner.runtime_handles_generation = None;
    inner.connecting = false;
    self.show_peers.clear_lv1(expected_generation);
    self.cue_lists_peers.clear_scenes();

    Some(RuntimeClearTransaction {
        cleared_generation: expected_generation,
        active_generation,
    })
}
```

After `advance_if_current` returns, do not introduce another await before all lifecycle state and peer mutation is complete.

Refactor `clear_runtime_transaction` to call this helper and publish only `ActiveGenerationChanged` on success. Refactor disconnect to publish disconnected then active-generation facts synchronously from the returned transaction and return unchanged on `None`.

Remove obsolete post-abort/finalization test hooks and rewrite their tests around the pre-transaction gate.

Run the three focused tests and the original rollback/stale peer tests. Expected: PASS.

- [ ] **Step 7: Carry reconnect attempt through connection completion**

Make `ConnectFailureMode::PreserveConnectedIdentity` carry `attempt: u64`, or add an equally narrow private completion context. `attempt_reconnect_lv1` must read connected identity and active reconnect attempt from one `ShowCommand::InitialProjectionState` reply. If reconnect is inactive, return a frontend-safe reconnect-unavailable error without starting a runtime.

Pass:

```rust
ConnectionCompletionMode::Reconnect { attempt }
```

for reconnect completion and `ConnectionCompletionMode::Unconditional` for manual/startup completion.

When Show replies with `accepted: false`, abort the rejected connection transaction and return a stale/superseded connection error before installing scene peers, logging `lv1_connected`, or remembering identity.

Add lifecycle/actor tests for rejected timed-out completion and accepted exact completion. Run focused filters and expect PASS.

- [ ] **Step 8: Make timeout cleanup cancellation-safe**

After Show accepts `ClaimReconnectTimeout`, spawn cleanup immediately with no intervening await:

```rust
let lifecycle = self.clone();
let cleanup = tauri::async_runtime::spawn(async move {
    lifecycle.disconnect_runtime_generation(generation).await
});
cleanup
    .await
    .map_err(|error| format!("Reconnect timeout cleanup task failed: {error}"))?
```

Add a test-only gate inside the spawned cleanup before the atomic transaction. Test:

- Show accepts the exact timeout attempt.
- Spawned cleanup reaches the gate.
- Abort/drop the outer timeout task.
- Release cleanup.
- Observe the disconnected and active-generation facts and closed old runtime handles.

Also add timeout-first and completion-first lifecycle actor tests. Timeout-first must reject later completion without connected logs or remembered identity; completion-first must make timeout return unchanged.

Run all focused timeout filters and expect PASS.

- [ ] **Step 9: Run complete verification**

Run:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p advanced-show-control lifecycle
cargo nextest run -p advanced-show-control show
cargo nextest run --workspace
cargo build --workspace
git diff --check
```

Expected: no formatting/clippy/build failures and every test passes.

- [ ] **Step 10: Inspect and commit**

Verify isolation before staging:

```bash
git rev-parse --show-toplevel
git branch --show-current
git status --short
git diff -- src-tauri/src/runtime/generation.rs src-tauri/src/lifecycle/mod.rs src-tauri/src/show src-tauri/src/ui/commands/lifecycle.rs
```

The toplevel must be `/Users/mixxorz/Projects/lv1-scene-fade-utility/.worktrees/issue-52-generation-rollback` and branch `fix/issue-52-generation-rollback`.

Stage only intended source/test files and commit:

```bash
git add src-tauri/src/runtime/generation.rs src-tauri/src/lifecycle/mod.rs src-tauri/src/show src-tauri/src/ui/commands/lifecycle.rs
git commit -m "fix: make disconnect cleanup atomic"
```

Confirm a clean worktree. Keep SDD reports under `.superpowers/sdd/` and untracked.

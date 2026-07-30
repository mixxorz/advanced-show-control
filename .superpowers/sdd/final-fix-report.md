# Issue #52 Final-Review Fix Report

## Status

Implemented all three final-review findings on `fix/issue-52-generation-rollback` in commit `abd9b7b` (`fix: serialize reconnect timeout cleanup`). The approved spec and plan were not modified. No frontend source changed.

## Root-cause evidence

### 1. Reconnect timeout identity and completion race

- `ui/src/AppRuntime.tsx` captures `appState.reconnect.attempt` and calls `services.reconnectTimedOut(attempt)`.
- `ui/src/commands.ts` forwards `{ attempt }` to Tauri.
- Before this fix, `src-tauri/src/ui/commands/lifecycle.rs::reconnect_timed_out` accepted no `attempt` argument and unconditionally called `disconnect_current_runtime()`.
- Therefore, timeout identity was discarded at the Rust adapter boundary. A delayed timeout had no way to distinguish its reconnect attempt from a later attempt or completed connection.
- Reconnect state is owned by the Show actor, while runtime generation and runtime handles are owned by `AppLifecycle`. Validation therefore needs both ownership boundaries without moving policy into the Tauri adapter.

### 2. Same-generation disconnect duplication

- The prior generation check and handle abort were atomic under the lifecycle mutex, but no cleanup owner was recorded.
- After the first request aborted handles and released the mutex, generation had not yet advanced. A second request for the same generation could pass the same check, report `changed: true`, publish another disconnected fact, and emit another success log.
- Deterministic RED evidence confirmed two changed results when the ownership check was removed: `left: 2, right: 1` in `overlapping_disconnects_for_one_generation_have_one_owner`.

### 3. Public-path stale cleanup regression

- Existing tests covered a private generation-scoped path and a later finalization gate, but did not gate the public `disconnect_current_runtime()` after generation capture and before cleanup.
- A new private, test-only pre-cleanup gate now exercises exactly that public path.
- With both current-generation checks deliberately disabled, the regression failed at `assert!(!disconnect.await.unwrap().changed)`, proving it detects stale public cleanup. Restoring the guards returned it to GREEN.

## Design chosen

### Lifecycle-owned atomic disconnect claim

`LifecycleInner` now records `disconnecting_generation: Option<u64>`.

`AppLifecycle` privately:

1. Captures the target generation.
2. Atomically checks that it is current and no disconnect is already claimed.
3. Records the generation claim under the lifecycle mutex.
4. Requires the same claim and current generation again when cleanup begins.
5. Releases the claim on stale/no-op/error paths and after successful finalization.

This makes cleanup ownership atomic and idempotent while keeping generation policy hidden from callers. No lifecycle mutex is held while awaiting the Show mailbox, avoiding lock inversion with connection completion.

### Show-owned timeout claim

A new explicit mailbox command, `ShowCommand::ClaimReconnectTimeout { attempt, reply }`, asks the Show actor to consume a timeout only when reconnect is active and its attempt exactly matches. The state transition (`active = false`) is the atomic claim, so a later timeout for the same attempt cannot succeed twice.

`AppLifecycle::reconnect_timed_out(attempt)` first claims the current lifecycle generation, then asks Show to claim that attempt. Outcomes:

- stale/mismatched/inactive attempt: release lifecycle claim, return `changed: false`, no disconnected fact or success log;
- connection completion reaches Show first: completion clears reconnect state, so timeout claim fails safely;
- timeout reaches Show first: timeout owns that attempt, then generation-scoped lifecycle cleanup proceeds;
- generation changes before cleanup: cleanup rechecks generation, releases the claim, and returns unchanged.

The Tauri adapter remains thin and now forwards `attempt` directly to the lifecycle owner.

## RED evidence

1. Timeout identity test initially failed compilation with:
   - `no method named reconnect_timed_out found for struct LifecycleTestFixture`
   - This established that no lifecycle timeout-policy interface existed and the adapter-discarded identity could not be validated.
2. Idempotency regression with the ownership exclusion deliberately removed:
   - command: `cargo nextest run -p advanced-show-control overlapping_disconnects_for_one_generation_have_one_owner --no-capture`
   - result: FAIL, assertion `left: 2, right: 1`.
3. Public stale-path regression with current-generation checks deliberately disabled:
   - command: `cargo nextest run -p advanced-show-control disconnect_current_runtime_does_not_touch_runtime_installed_before_cleanup --no-capture`
   - result: FAIL at `assert!(!disconnect.await.unwrap().changed)`.

The deliberate RED mutations were restored immediately and are not present in the commit.

## GREEN evidence

Focused tests passed:

- `reconnect_timeout_for_mismatched_attempt_is_unchanged`
- `reconnect_completion_wins_before_timeout_claim`
- `overlapping_disconnects_for_one_generation_have_one_owner`
- `disconnect_current_runtime_does_not_touch_runtime_installed_before_cleanup`
- `reconnect_timeout_claim_requires_the_active_attempt`

The actor/lifecycle tests use lifecycle operations, Show mailboxes, `AppEventBus`, tracing capture, and oneshot gates. They use no timing sleeps and do not mutate actor internals.

## Verification commands and results

Baseline:

- `cargo nextest run -p advanced-show-control lifecycle`
  - 31 passed before final-review changes.

Final required verification:

```text
cargo nextest run -p advanced-show-control lifecycle
  35 passed, 0 failed
cargo nextest run -p advanced-show-control show
  56 passed, 0 failed
cargo fmt --all -- --check
  exit 0
cargo clippy --workspace --all-targets -- -D warnings
  exit 0, no warnings
cargo build --workspace
  exit 0
```

Final focused public-path rerun after restoring RED mutation:

```text
cargo nextest run -p advanced-show-control disconnect_current_runtime_does_not_touch_runtime_installed_before_cleanup --no-capture
  1 passed, 0 failed
git diff --check
  exit 0
git status --short
  clean
```

Commit hooks also ran and passed Cargo fmt and Cargo clippy.

## Files changed

- `src-tauri/src/lifecycle/mod.rs`
  - lifecycle disconnect claim/serialization;
  - reconnect timeout lifecycle policy;
  - deterministic pre-cleanup test gate;
  - lifecycle/actor regression tests.
- `src-tauri/src/show/commands.rs`
  - explicit reconnect-timeout claim mailbox command.
- `src-tauri/src/show/actor.rs`
  - timeout claim command handling.
- `src-tauri/src/show/state.rs`
  - exact active-attempt claim transition and unit coverage.
- `src-tauri/src/show/mod.rs`
  - test-only re-export for actor fixture construction.
- `src-tauri/src/ui/commands/lifecycle.rs`
  - thin adapter now accepts and forwards `attempt`.

No frontend files, approved spec, or approved plan changed.

## Commits

- `abd9b7b fix: serialize reconnect timeout cleanup`

Pre-existing issue #52 commits retained unchanged:

- `0ac3201 test: exercise stale disconnect actor peers`
- `eb6ea96 fix: scope disconnect cleanup by generation`
- `a1dd4bc fix: keep disconnect generation monotonic`

## Self-review

- Generation comparison, disconnect ownership, and stale/no-op behavior remain private to `AppLifecycle`.
- Reconnect-attempt identity remains owned and atomically consumed by the Show actor.
- The Tauri adapter contains no validation or policy.
- No lifecycle mutex is held over a Show mailbox await, avoiding a connection-completion lock-order inversion.
- Every path after acquiring the lifecycle timeout claim releases it on Show mailbox failure, rejected attempt, stale cleanup, or successful completion.
- Stale/no-op paths emit no disconnected fact and no `lv1_disconnected` INFO success log.
- Existing generation guards, peer generation matching, actor mailbox boundaries, lockout, exact scene identity, and fader safety paths were not weakened.

## Concerns / remaining risk

- Hardware smoke testing was not run because this change is deterministic lifecycle/actor policy and the smoke suite requires an LV1-compatible target environment.
- The successful timeout-vs-completion winner is defined by Show mailbox ordering after the lifecycle claim: completion first rejects timeout; timeout first consumes the attempt and proceeds with generation-guarded cleanup. Tests cover completion-first, mismatch, overlap, and stale-generation cleanup.
- The Show timeout claim intentionally does not publish an intermediate projection. The authoritative disconnected fact drives the existing final connection metadata update; stale/no-op requests publish no misleading state fact.

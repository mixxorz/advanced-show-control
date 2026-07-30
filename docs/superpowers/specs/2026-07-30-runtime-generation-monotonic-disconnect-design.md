# Monotonic Runtime Generation During Disconnect Design

**Issue:** GitHub #52 — prevent runtime generation rollback during disconnect

## Context

`AppLifecycle` owns connected-runtime setup and teardown. Its shared `RuntimeGeneration` is cloned into generation-sensitive actors so stale tasks can reject fader writes and other runtime activity after disconnect or reconnect.

The current disconnect path captures generation `N`, releases the lifecycle lock, aborts runtime handles, clears peers, and writes the captured value back through `RuntimeGeneration::set`. A concurrent connection can advance the shared generation before that final write. The stale disconnect can then restore `N`, making generation non-monotonic and allowing later cleanup to treat stale state as current.

Removing the write alone is insufficient. The unscoped abort operation can also resume after a newer runtime is installed and abort that newer runtime's handles. The disconnect cleanup itself must remain bound to the generation that the request originally targeted.

## Goals

- Make production runtime generation mutation monotonic.
- Ensure a disconnect request can affect only the generation that was current when the request began.
- Prevent stale disconnect work from aborting or clearing newer runtime handles and peers.
- Preserve normal disconnect event ordering, peer cleanup, handle cleanup, and generation advancement.
- Keep concurrency policy inside `AppLifecycle` rather than exposing it to callers.
- Cover the previously unsafe interleaving deterministically without timing sleeps.

## Non-Goals

- Redesign connected-runtime construction or actor ownership.
- Change generation filtering in fade, scene recall, projection, or settings code.
- Address settings file I/O under the generation guard; that remains GitHub #54.
- Address cue-list scene identity across reconnect; that remains GitHub #66.
- Change lockout, exact scene identity, fade abort, manual override, or LV1 write policy.

## Design

### Runtime generation interface

`RuntimeGeneration` will expose only monotonic production mutation:

- `current` reads the active generation.
- `advance` increments the active generation and returns it.
- `if_current` retains its existing guarded-operation behavior until #54 changes that interface.
- Arbitrary assignment through `set` will be unavailable to production code. Existing test fixtures may retain a test-only assignment helper under `#[cfg(test)]`.

This makes rollback unrepresentable through the production `RuntimeGeneration` interface.

### Generation-scoped disconnect cleanup

The public lifecycle interface remains `disconnect_current_runtime()`. Callers do not receive or manage generation tokens for cleanup.

`disconnect_current_runtime()` will:

1. Capture the generation that the request targets.
2. Attempt private generation-scoped handle cleanup.
3. Continue only if the captured generation is still current.
4. Abort handles owned by that lifecycle state and clear its ownership marker.
5. Clear the generation-matched Show LV1 peer.
6. Publish the existing LV1 disconnected fact for the captured generation.
7. Run the existing generation-scoped runtime clear transaction, which clears remaining peers, advances generation, and publishes the active-generation change.
8. Return `changed: true` and retain the existing user-facing disconnect log.

The private cleanup operation replaces or narrows `abort_runtime_handles_without_advancing_generation`. It accepts the expected generation internally and performs the current-generation check while holding the lifecycle lock. It does not write runtime generation.

If a newer connection advances generation before cleanup acquires the lifecycle lock, the cleanup returns a stale/no-change outcome. The public disconnect command then:

- does not abort handles,
- does not clear peers,
- does not publish a misleading disconnected fact,
- does not emit the successful user-facing disconnect log, and
- returns `changed: false`.

This stale outcome is an idempotent no-op rather than an error. A superseded disconnect does not require recovery by its caller.

### Information hiding

Generation comparison, runtime-handle ownership, and stale cleanup behavior remain private to `AppLifecycle`. No new public helper asks command adapters or other actors to coordinate generation checks. This keeps the lifecycle module's interface small and prevents callers from accidentally separating validation from cleanup.

The deterministic test coordination seam will be test-only and private. It will pause the disconnect after generation capture and before cleanup so the unsafe interleaving can be reproduced without exposing synchronization controls in production.

## Concurrency Behavior

The regression test will force this ordering:

1. Runtime generation `N` is active.
2. A disconnect captures `N` and pauses at a test-only gate.
3. Connection work advances to a newer generation and installs newer runtime handles.
4. The disconnect resumes with expected generation `N`.
5. Generation-scoped cleanup detects that `N` is stale and performs no mutation.

The active generation remains newer than `N`, and the newer runtime handles and peers remain usable. No production path writes a previously captured generation back into shared state.

Normal, uncontended disconnect behavior remains unchanged: handles are aborted before the disconnected fact is published, peers are cleared, generation advances once, and the active-generation fact follows the LV1 disconnected fact.

## Error Handling and Logging

A superseded disconnect is a safe no-op, not a command failure. It returns `changed: false`. Any diagnostic for this branch should be `DEBUG`, because the newer lifecycle operation is already authoritative and no engineer action is required.

A successful disconnect retains the existing complete `INFO` message. The implementation must not emit that message for stale cleanup because doing so would misrepresent the newer runtime's state.

No new frontend error strings or projected state fields are required.

## Testing

### Lifecycle/actor-style regression test

Use lifecycle operations, fake actor handles, `AppEventBus`, and explicit test gates. Do not use timing sleeps or directly mutate actor internals.

The test will prove that a disconnect paused after capturing generation `N` cannot, after a newer generation is installed:

- reduce the active generation,
- abort the newer LV1 or fade targets,
- clear newer generation-matched peers,
- publish a stale successful disconnect sequence, or
- report that it changed the active runtime.

The newer handle will be exercised through its mailbox or an existing lifecycle-access path so survival is verified behaviorally rather than only by inspecting private fields.

### Existing lifecycle behavior

Retain or extend lifecycle tests proving:

- sequential generation allocation is monotonic,
- a normal disconnect publishes the LV1 disconnected fact followed by the advanced generation fact,
- normal cleanup clears runtime ownership and peers, and
- stale runtime installation and finalization cannot replace newer peers.

### Pure unit coverage

Use pure unit coverage for `RuntimeGeneration` only where it adds behavioral value, such as proving repeated `advance` calls never decrease the value. Production compilation and the absence of a production setter enforce the stronger API invariant.

## Verification

Use the repository's required Rust workflow:

```bash
cargo nextest run -p advanced-show-control lifecycle
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Run broader Rust or repository verification if implementation changes affect code outside lifecycle and runtime generation.

## Documentation Impact

`docs/architecture.md` already requires generation guards and stale-runtime rejection. Update it only if implementation changes the documented lifecycle sequence or public architecture. No frontend or user-manual documentation change is expected.

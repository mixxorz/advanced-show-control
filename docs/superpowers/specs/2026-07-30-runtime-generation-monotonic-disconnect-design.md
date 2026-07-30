# Atomic Monotonic Runtime Disconnect Design

**Issue:** GitHub #52 — prevent runtime generation rollback during disconnect

## Context

`AppLifecycle` owns connected-runtime setup and teardown. Its shared `RuntimeGeneration` is cloned into generation-sensitive actors so stale tasks reject fader writes and other runtime activity after disconnect or reconnect.

The original disconnect path could capture generation `N`, release the lifecycle lock, and later write `N` back after another operation advanced generation. The first implementation removed that write and added a manual `disconnecting_generation` claim. Final review showed that a claim held across awaits introduced new failure modes: cancellation could leak the claim, an old claim could block a newer generation, and reconnect timeout arbitration remained split across lifecycle and Show state.

The revised design removes long-lived claims. Advancing the current generation is the atomic, cancellation-safe ownership operation for runtime cleanup.

## Goals

- Make production runtime generation mutation monotonic.
- Ensure cleanup affects only the generation targeted by its request.
- Make same-generation disconnect requests idempotent.
- Prevent stale work from blocking or aborting newer generations.
- Avoid ownership state that can leak when an async caller is cancelled.
- Resolve reconnect timeout versus connection completion by exact reconnect attempt identity.
- Preserve normal disconnect fact ordering and complete user-facing logs.
- Keep policy in `AppLifecycle`, `RuntimeGeneration`, and the Show actor rather than Tauri or React adapters.
- Cover unsafe interleavings deterministically without timing sleeps.

## Non-Goals

- Redesign the full connected-runtime actor graph.
- Change fade, scene recall, lockout, exact identity, or LV1 write policy.
- Address settings file I/O under the generation guard; that remains GitHub #54.
- Address cue-list scene identity across reconnect; that remains GitHub #66.
- Add frontend reconnect behavior or new projected fields.

## Design

### Runtime generation interface

`RuntimeGeneration` exposes these production operations:

- `current` reads the active generation.
- `advance` allocates the next generation for an unconditional lifecycle transition.
- `advance_if_current(expected)` atomically compares the current generation with `expected`, advances only on equality, and returns the new generation.
- `if_current` retains its existing guarded-operation behavior until #54 narrows that interface.

Arbitrary assignment remains test-only. `advance_if_current` acquires the generation mutex once; cancellation before acquisition makes no mutation, and successful return means the generation has already advanced monotonically.

### Atomic runtime clear transaction

`AppLifecycle` owns a private generation-scoped clear transaction. While holding the lifecycle mutex, it:

1. Calls `RuntimeGeneration::advance_if_current(expected_generation)`.
2. Returns a stale/no-change result if the comparison fails.
3. After successful advancement, performs only synchronous work before releasing the lifecycle mutex:
   - abort runtime handles,
   - clear the runtime ownership marker,
   - clear connection-in-progress state,
   - clear the generation-matched Show LV1 peer, and
   - clear the Cue Lists scenes peer.
4. Returns the old and newly active generations as a private transaction result.

Generation advancement happens before handle mutation because it is the transaction's only remaining await. Once it succeeds, Rust cannot cancel the future between the state mutations because there is no later await in the critical section.

No `disconnecting_generation` field or release protocol remains.

### Event publication

The transaction mutates safety state before publishing facts. Callers publish facts synchronously after the transaction returns:

- A normal runtime clear publishes `ActiveGenerationChanged(new_generation)`.
- A user-visible disconnect publishes `Lv1Event::Disconnected` for the old generation, then `ActiveGenerationChanged(new_generation)`, then logs `lv1_disconnected` at `INFO` and returns `changed: true`.
- A stale or duplicate disconnect publishes no disconnected fact, no active-generation fact, and no success log; it returns `changed: false` with an optional `DEBUG` diagnostic.

The shared guard may already contain the new generation when consumers receive the old-generation disconnected fact. This is intentional: safety guards become stale before any task can run again, while broadcast ordering remains compatible with the projector and Show actor.

### Same-generation and cross-generation behavior

Two requests that captured generation `N` race through `advance_if_current(N)`. Exactly one can advance to `N+1`; the other observes a mismatch and becomes a no-op.

A request for stale generation `N` never blocks a request for current generation `N+1`. There is no persistent ownership record shared between generations.

### Reconnect timeout identity

The frontend already supplies `ReconnectState.attempt` to the Tauri timeout command. The adapter accepts `attempt` and forwards it unchanged to `AppLifecycle`.

The Show actor remains authoritative for reconnect attempt state. Its timeout command atomically claims only an active, exact attempt and records that the attempt timed out. Connection completion for a reconnect carries the same attempt identity and is accepted only while that attempt remains active and unclaimed. Manual and startup connections use an unconditional completion mode rather than inventing reconnect attempts.

`AppLifecycle::attempt_reconnect_lv1` obtains the connected identity and active reconnect attempt from one Show projection snapshot. The attempt travels with the connection transaction to Show completion.

The Show actor returns an explicit accepted/rejected completion outcome; `changed: false` is not overloaded to mean rejection. A rejected completion cannot log a successful connection, remember the identity, or install the scene peer.

### Timeout cleanup and cancellation

`AppLifecycle::reconnect_timed_out(attempt)`:

1. Captures the current generation.
2. Asks Show to claim the exact timeout attempt.
3. Returns unchanged if Show reports inactive, mismatched, completed, or previously claimed.
4. After a successful Show claim, immediately spawns generation-scoped atomic cleanup as an owned Tokio task and awaits its result.

There is no await between receiving a successful Show claim and spawning cleanup. Dropping the outer Tauri command future therefore does not cancel cleanup; dropping a Tokio `JoinHandle` detaches the spawned task.

Race outcomes are defined by Show mailbox ordering:

- Completion first: Show completes and clears the attempt; timeout claim is rejected and does not disconnect.
- Timeout first: Show records the timeout; later completion for that attempt is rejected, while detached generation-scoped cleanup invalidates that runtime.
- Generation changes before cleanup: cleanup is stale and does not touch the newer runtime. The timed-out reconnect completion remains rejected by attempt identity.

### Information hiding

Callers use `disconnect_current_runtime()`, `reconnect_timed_out(attempt)`, and connection commands. They do not coordinate generation checks, transaction ownership, or reconnect completion policy.

Tauri adapters deserialize and forward values only. React continues to pass the projected reconnect attempt without duplicating backend policy.

## Error Handling and Logging

Stale, duplicate, inactive, and mismatched disconnect/timeout requests are safe no-ops, not command failures. They return `changed: false` and may emit `DEBUG` diagnostics.

Mailbox closure and task join failures remain command errors with frontend-safe messages. A successful timeout claim whose detached cleanup continues after caller cancellation does not require a caller to recover safety state.

Successful normal disconnect retains the existing complete `INFO` message. Rejected connection completion must not emit `lv1_connected` or remembered-identity persistence errors.

## Testing

### Pure unit tests

Test `RuntimeGeneration::advance_if_current` directly:

- matching expected generation advances once,
- stale expected generation returns no change,
- repeated matching attempts cannot both succeed.

Test Show state policy directly:

- only the active exact timeout attempt can be claimed,
- duplicate or mismatched timeout claims fail,
- completion first rejects timeout,
- timeout first rejects completion for the same attempt,
- unconditional manual completion remains valid.

### Lifecycle/actor tests

Use lifecycle commands, Show/Cue Lists/Scenes/LV1/Fade mailboxes, `AppEventBus`, tracing capture, and explicit gates.

Cover:

- the original generation rollback interleaving,
- a newer runtime installed after public disconnect capture but before atomic cleanup,
- two same-generation disconnects producing exactly one changed result and one success fact/log sequence,
- a stale generation never suppressing a newer generation's disconnect,
- cancellation of the outer timeout future after Show claim while detached cleanup still completes,
- completion-first timeout ordering,
- timeout-first completion rejection,
- mismatched and inactive timeout attempts,
- newer runtime handles and actor peer routing remaining usable after stale cleanup, and
- normal disconnected-then-active-generation event order.

Tests must not use sleeps or directly mutate side-effecting actor internals.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p advanced-show-control lifecycle
cargo nextest run -p advanced-show-control show
cargo nextest run --workspace
cargo build --workspace
```

Hardware smoke is not required because this change does not alter LV1 protocol behavior and the smoke target requires an LV1-compatible environment.

## Documentation Impact

`docs/architecture.md` already requires monotonic generation safety and stale-runtime rejection. Update it only if implementation changes the documented lifecycle sequence or public architecture. No frontend or user-manual change is expected.

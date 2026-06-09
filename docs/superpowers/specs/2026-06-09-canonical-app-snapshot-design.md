# Canonical App Snapshot Design

## Purpose

Prevent UI state regressions where a shell transition returns an `AppViewState` built from shell-owned state only and silently replaces real show data with `ShowSnapshot::empty()`.

The recent empty Scenes tab bug came from a production command returning a later incomplete snapshot after the shell had already emitted a correct full snapshot. The fix should make this class of bug hard to reintroduce.

## Scope

This refactor is limited to Tauri shell view-state projection. It does not change LV1 ownership, show-file semantics, scene recall policy, fade safety rules, diagnostics, or frontend rendering behavior.

## Design

`ShellState` should have one canonical way to build UI-facing `AppViewState` values:

- Read shell-owned fields from `ShellInner`.
- Drop the `ShellInner` lock.
- Read the real `ShowSnapshot` from `ShowStateHandle`.
- Combine both parts into `AppViewState`.

Production methods that mutate shell state and return UI state should call `snapshot()` or `snapshot_for_generation()` after mutation. If generation validation matters, they should use the generation-aware path so stale tasks still fail closed.

`snapshot_from_inner` should not remain as a way to produce `AppViewState` with an empty show snapshot. If an inner helper is still useful, it should only produce shell-owned intermediate data and require an explicit real `ShowSnapshot` before an `AppViewState` can be constructed.

Tests should exercise the same UI snapshot path used by production. If a helper exists only to support an obsolete incomplete UI state shape, remove it rather than preserving test-only behavior the app should never return.

## Data Flow

For shell-only transitions:

1. Lock `ShellInner`.
2. Apply the mutation.
3. Capture any generation or reconciliation data needed after the lock is released.
4. Drop the lock.
5. Return `snapshot()` or `snapshot_for_generation(generation)`.

For transitions that also touch `ShowState`:

1. Apply the show mutation through `ShowStateHandle`.
2. Apply any shell metadata mutation, such as dirty state or selected scene.
3. Return the canonical full snapshot.

For LV1 event projection:

1. Validate the active generation before mutating shell state.
2. Drop the shell lock before awaiting `ShowStateHandle`.
3. Re-check generation before applying delayed results.
4. Return `snapshot_for_generation(generation)` when the event belongs to a guarded runtime task.

## Error Handling And Safety

Generation guards remain unchanged. Stale runtime events must still return `None` and must not overwrite current UI state.

Show snapshot failures should use the existing behavior of the caller path. The unguarded `snapshot()` path may continue to fall back to an empty show snapshot for status retrieval if that is the existing recovery behavior, but production transition helpers should not bypass `ShowStateHandle` by constructing an empty show snapshot themselves.

No lock should be held while awaiting `ShowStateHandle`, to avoid deadlocks between shell projection and show reconciliation.

## Testing

Add or update tests so they verify returned `AppViewState` values preserve real show scene configs across production transitions that previously could return shell-only snapshots.

Relevant coverage:

- Connected identity establishment includes scene configs.
- Pending identity changes preserve scene configs.
- Connect failure preserves scene configs while clearing connection state.
- LV1 disconnect/reconnect projection preserves scene configs.
- Scene list projection continues to avoid deadlock and includes reconciled show configs.

Remove tests that only validate incomplete `AppViewState` construction. Tests should not preserve behavior the app no longer uses.

## Verification

Use targeted checks first:

```bash
cargo test -p advanced-show-control-tauri app_state::shell::tests
cargo test -p advanced-show-control-tauri app_state::events_tests
cargo test -p advanced-show-control-tauri commands::tests
```

Before completion, run formatting and the smallest broader verification that proves the refactor:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

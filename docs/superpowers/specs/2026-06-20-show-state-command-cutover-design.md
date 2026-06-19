# Show State Command Cutover Design

## Context

The previous single-crate command/projection architecture spec established the target direction:

- `ShellState` should be eliminated.
- `AppCommandBus` should be the app command boundary.
- Tauri commands should become thin adapters.
- The projector should be the only `app-status-changed` emitter.
- `show/` should own broad app/session state for now.

The incremental migration path has created too much transitional code. Preserving a working intermediate shell state while moving one field at a time forces temporary forwarding, duplicate state updates, and projector fixes that are deleted shortly afterward.

This refactor should be a single cutover. The branch may be temporarily broken while the move is in progress, but the committed result must pass verification and preserve runtime safety behavior.

## Decision

Move all `ShellState` source-of-truth state to its final owners in one refactor:

- `show/` owns app/session state.
- `AppLifecycle` owns runtime lifecycle state.
- `lv1/`, `fade/`, and `logging/` keep their existing domain ownership.
- `projector/` owns only a coalescing projection cache.

Remove `ShellState` instead of adding a temporary compatibility facade.

## Ownership Boundaries

### `show/`

`show/` owns app/session state:

- lockout
- scene configs
- cued scene
- selected scene
- show-file path/name/dirty/last-saved metadata
- discovered LV1 systems
- connected and pending LV1 identity metadata used by the UI
- reconnect UI metadata
- show/session command result state

`ShowState` is the source of truth for this state. It is not a DTO and should not be named or treated as a snapshot. Its fields must be private.

`show/` does not own copies of LV1, fade, or log state.

### `AppLifecycle`

`AppLifecycle` owns runtime lifecycle state:

- generation
- current `AppCommandBus`
- LV1 actor handle/task
- fade engine handle/task
- scene recall fader task
- projector task
- lifecycle/show monitor tasks, if still needed after the cutover
- stale-runtime installation and cleanup
- disconnect/reconnect runtime abort behavior

`ActiveCommandBus` should be removed. Tauri commands should access the current command bus through `AppLifecycle`.

### `lv1/`, `fade/`, and `logging/`

These modules remain source owners for their domains:

- `lv1/` owns live console mirror state and LV1 protocol behavior.
- `fade/` owns active fade execution state and fader writes.
- `logging/` owns tracing setup and the UI log channel.

No LV1 protocol behavior changes are part of this refactor.

### `projector/`

The projector owns a cache only so it can coalesce changes and emit `AppViewState` at 10 Hz when dirty. It is not a source-of-truth owner.

## Command And Effects Model

`show::commands` are the authoritative implementation of app/session commands, not wrappers around shell-state mutation helpers.

Command handlers should perform the complete command transaction directly:

- validate inputs
- mutate private `ShowState`
- update dirty/file metadata
- publish `ShowEvent`
- emit operational `tracing` logs
- return command-specific results

Behavior that used to be described as a `ShellState` side effect should become direct command-handler implementation whenever it is synchronous and part of the command's meaning.

Examples:

- `new_show_file` clears/reconciles show data, selects the first scene when appropriate, clears file metadata, marks the show clean, logs creation, and publishes the new show state.
- `load_show_file_from_dto` imports and validates against LV1 state, replaces show data, stores file metadata, marks dirty if import pruned anything, logs skipped scenes, and publishes the new show state.
- `save_show_file` export/mark-saved behavior updates show-file metadata inside the command path after IO succeeds.
- scene duration, cue, scope, and store commands mark the show dirty only when the command changes source state.

Use event listeners only for behavior that is genuinely event-driven or cross-cutting, such as reacting to LV1 scene-list facts or lifecycle disconnect facts. Do not use listeners to recreate synchronous command side effects.

## Public State Access

State stored in `show/` should be private.

`ShowStateHandle` should not expose broad public mutation setters such as direct metadata setters, `replace_snapshot`, `mark_show_file_dirty`, or direct field mutation helpers.

Allowed access patterns:

- `show::commands` may mutate private `ShowState` through module-private handle methods or direct same-module helpers.
- `AppCommandBus` exposes command methods that call `show::commands`.
- Tauri commands call `AppCommandBus`, not `ShowStateHandle`.
- Read-only queries may exist through `AppCommandBus` for non-mutating consumers that need current source state, such as scene recall validation.

The implementation should prefer command-oriented APIs over setter APIs. If a lower-level mutation helper remains, it should be private to `show/` and should not be callable from Tauri commands or other modules.

## Event And Projection Model

`AppViewState` remains the frontend state contract for this refactor.

The only backend emitter of `app-status-changed` is the projector.

The projector has exactly two live inputs:

1. `AppEventBus`
2. UI logging channel

There is no projection dirty notifier outside these inputs.

The projector cache is dirty when an event or log arrives. It emits at the existing 10 Hz cadence.

### Show Events

Show events must carry the current show-owned projection payload needed by the projector. The projector applies that payload with an `apply_show_state`-style cache update and does not pull state from `show`.

`ShowEvent` should include:

- a reason enum for diagnostics/tests
- the current show-owned state payload needed for `AppViewState`

This payload is a projection payload derived from `ShowState`; it is not the source-of-truth `ShowState` itself.

### LV1 And Fade Events

LV1 and fade events update projector cache fields directly from event payloads because their source owners publish enough facts to maintain the UI projection.

### Logs

UI logs flow through:

```text
tracing -> logging channel -> projector cache -> app-status-changed
```

Logs do not go through `AppEventBus` and are not stored in `show`.

## Tauri And React Boundaries

Tauri commands become thin wrappers:

- deserialize frontend arguments
- get the current `AppCommandBus` from `AppLifecycle`
- call the corresponding command-bus method
- map errors and command-specific results to the frontend response

Tauri commands must not:

- mutate `ShowState` directly
- call `ShowStateHandle` mutation methods
- build `AppViewState`
- emit `app-status-changed`
- own business or safety validation

React updates app state only from `app-status-changed`. Command return values are command-specific results for control flow and user feedback, not app state snapshots.

## Runtime Lifecycle Cutover

Move `RuntimeHandles` and generation logic out of `ShellState` and into `AppLifecycle`.

`AppLifecycle` should provide explicit operations for:

- beginning a connection attempt and allocating the next generation
- installing connected runtime handles for the active generation
- rejecting stale runtime handles
- exposing the current command bus
- aborting the current runtime
- clearing runtime handles for a matching generation
- clearing reconnect/runtime state on disconnect

Disconnect cleanup remains generation-guarded. Stale tasks must not clear current runtime state or send commands after disconnect/reconnect.

## Show-Owned Connection Metadata

Connection UI metadata currently stored in shell state moves to `show/`:

- discovered LV1 systems
- connected LV1 identity
- pending LV1 identity
- reconnect UI metadata

Commands and lifecycle operations that change this metadata should publish `ShowEvent` with the updated show-owned projection payload.

`lv1/` still owns the live connection mirror. `show/` owns only app/UI metadata derived from discovery, connect intent, connected identity, and reconnect status.

## Show-File Ownership

Show-file DTO import/export, validation, selected-scene restoration, file metadata, dirty tracking, and operational logs are handled through `show::commands`.

Native dialogs and physical file IO may remain adapter/infrastructure responsibilities, but command handlers own the state transaction around successful load/save/new operations.

The saved show-file format remains unchanged unless a separate design explicitly changes it.

## Scene Recall Safety

UI-requested scene recall remains routed through `AppCommandBus` into show-owned command logic.

The command path must preserve:

- lockout checks
- stored scene config lookup
- connected LV1 state requirement
- exact scene identity validation
- LV1 recall dispatch through the command boundary
- visible blocked/failed outcomes through logs or returned command errors

LV1-observed scene recall automation remains owned by `scene_recall/` and must preserve fresh LV1 state checks, generation guards, skipped/blocked behavior, and fade safety semantics.

## Implementation Shape

The implementation should be a single cutover, not a compatibility-preserving sequence.

Expected structural changes:

- Expand private `ShowState` to include all show-owned app/session fields.
- Add a show-owned projection payload type for events and projector cache updates.
- Change `ShowEvent` to carry the current show-owned projection payload.
- Move command logic from `ShellState` and Tauri command handlers into `show::commands`.
- Move runtime handles and generation logic into `AppLifecycle`.
- Remove `ShellState` and its tests or rewrite tests against `show::commands`, `AppCommandBus`, `AppLifecycle`, and projector cache.
- Update `AppCommandBus` to route all app/session commands and queries.
- Update projector cache to apply show state from events, not pull from show.
- Update React command handling so command returns are not applied as app state.
- Remove direct command/log emits of `app-status-changed`.

The branch does not need to compile at every intermediate edit. The final commit must compile, pass tests, and preserve safety behavior.

## Guardrails

Add or update tests/static checks for these invariants:

- `ShellState` is removed.
- `ActiveCommandBus` is removed.
- `ShowState` fields are private.
- Tauri commands do not call `ShowStateHandle` mutation methods.
- Mutating Tauri commands do not return `AppViewState`.
- React does not apply command results as app state snapshots.
- Only projector code emits `app-status-changed`.
- Projector does not pull state from `show`; it applies show state delivered by `ShowEvent`.
- Show/app mutations publish `ShowEvent` carrying current show-owned projection payload.
- Logs reach UI only through projector logging input.
- UI-requested recall validation remains out of Tauri adapters.
- CLI/probe binary remains buildable.

## Verification

Before claiming the cutover complete, run the relevant targeted tests and then the broad checks:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo build --workspace
```

Also run frontend checks after command contract changes:

```bash
npm --prefix ui run typecheck
npm --prefix ui run test
```

Run the preserved probe binary build or full workspace build to ensure `lv1-probe` remains valid.

## Non-Goals

- No frontend `AppViewState` schema redesign.
- No saved show-file format redesign.
- No immediate extraction of `scenes/`, `sessions/`, or `connection/` modules from `show/`.
- No LV1 protocol behavior changes.
- No changes to `lv1/` live mirror ownership.
- No changes to `fade/` execution ownership.
- No routing logs through `AppEventBus`.
- No weakening of lockout, exact scene identity, generation guards, disconnect behavior, manual override, abort, overlap, or same-scene safety behavior.

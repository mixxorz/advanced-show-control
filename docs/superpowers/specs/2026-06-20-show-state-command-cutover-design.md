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
- `AppEventBus` is app-lifetime state shared by show, lifecycle/runtime actors, and projector.

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
- app-lifetime `AppCommandBus`
- LV1 actor handle/task
- fade engine handle/task
- scene recall fader task
- projector task
- lifecycle/show monitor tasks, if still needed after the cutover
- stale-runtime installation and cleanup
- disconnect/reconnect runtime abort behavior

`ActiveCommandBus` should be removed. Tauri commands should access the app-lifetime command bus through `AppLifecycle`.

### `lv1/`, `fade/`, and `logging/`

These modules remain source owners for their domains:

- `lv1/` owns live console mirror state and LV1 protocol behavior.
- `fade/` owns active fade execution state and fader writes.
- `logging/` owns tracing setup and the UI log channel.

No LV1 protocol behavior changes are part of this refactor.

### `projector/`

The projector owns a cache only so it can coalesce changes and emit `AppViewState` at 10 Hz when dirty. It is not a source-of-truth owner.

The projector owns `AppViewState.state_version`. Versions are projection sequence numbers, not show/runtime generation values. They start above the frontend's initial `0` and increase every time the projector emits an `AppViewState`.

## Event Bus Lifetime

There should be one app-lifetime `AppEventBus` managed at app setup and shared with:

- `ShowStateHandle`
- `AppLifecycle`
- the active LV1 actor
- the active fade engine
- scene recall automation
- projector
- any event-driven monitors that remain after the cutover

Runtime connection attempts must not create a disconnected event bus that isolates LV1/fade events from show events. Runtime actors may subscribe/publish for the active runtime generation, but they use the app-lifetime bus.

There should also be one app-lifetime `AppCommandBus` managed by `AppLifecycle`. The command bus is not recreated on reconnect or disconnect. It keeps show/app/session command routing available while offline, and reconnect/disconnect operations only install or clear generation-scoped LV1/fade runtime targets. Correctness relies on generation checks, not replacing the command bus.

This is required because show command handlers publish `ShowEvent` facts and the projector must observe those facts without polling `show`.

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

The projector and its subscriptions are constructed during app setup, before runtime event producers start. It owns the initial disconnected projection and emits the first `AppViewState` through `app-status-changed` only after the frontend has registered its listener and signaled readiness. Startup, connect, reconnect, discovery, show-file, and command flows update source owners and publish events; they do not return or directly emit full app state.

`AppEventBus` remains a lossy broadcast facts bus with no replay. This cutover assumes the projector does not lag past show events. Lag recovery for `ShowProjectionState` is deferred. Do not add watch channels, resync request events, projector pull exceptions, or replay-last event-bus behavior in this refactor. Existing lag handling should remain diagnostic only.

### Show Events

Show events must carry the current show-owned projection payload needed by the projector. The projector applies that payload with an `apply_show_state`-style cache update and does not pull state from `show`.

Define a show-owned projection DTO such as `ShowProjectionState` with exactly the show-owned fields needed by `AppViewState`:

- `lockout`
- `scene_configs`
- `cued_scene_id`
- `selected_scene_id`
- `show_file_path`
- derived or stored show-file display name
- `show_file_dirty`
- `show_file_last_saved_at`
- `discovered_lv1_systems`
- `connected_lv1_identity`
- `pending_lv1_identity`
- `reconnect`
- `last_event_at`, if retained as a UI field

`ShowProjectionState` is a DTO derived from private `ShowState`. It is not the source-of-truth state type.

`ShowEvent` includes:

- a reason enum for diagnostics/tests
- `state: ShowProjectionState`

Every show/app mutation that affects UI state publishes a `ShowEvent` carrying the full current `ShowProjectionState`. This avoids partial event replay and lets the projector replace its show-owned cache in one operation.

The projector cache exposes `apply_show_state(state: ShowProjectionState)`. It must not call `ShowStateHandle`, `show.get_*`, `show.snapshot`, or any equivalent show read path.

### LV1 And Fade Events

Runtime-originated events, including LV1, fade, and scene-recall events, must carry the runtime generation that produced them. Lifecycle publishes an app-lifetime runtime-generation event whenever a new runtime generation becomes active. The projector and show-owned runtime listeners track that active runtime generation internally and ignore runtime events from stale generations.

LV1 and fade events update projector cache fields directly from event payloads because their source owners publish enough facts to maintain the UI projection. Direct LV1/fade projection is limited to LV1/fade-owned UI fields:

- LV1 connection status
- current LV1 scene
- LV1 scene list
- LV1 channel topology and live values needed for projection
- fade state

LV1/fade events must not update show-owned connection metadata such as discovered systems, connected identity, pending identity, reconnect state, selected scene, show-file metadata, or lockout/config/cue state.

Disconnected handling is not a projector side effect. For an active-generation `Disconnected` fact, the projector updates only LV1-owned projection fields. `show/` listens to the same generated disconnected fact, validates it against its internally tracked active runtime generation, updates show-owned connection UI metadata through private `ShowState`, and publishes `ShowEvent { state: ShowProjectionState }` for the projector to apply. Stale disconnect facts must not clear show-owned connection metadata.

### Logs

UI logs flow through:

```text
tracing -> logging channel -> projector cache -> app-status-changed
```

Logs do not go through `AppEventBus` and are not stored in `show`.

## Tauri And React Boundaries

Tauri commands become thin wrappers:

- deserialize frontend arguments
- get the app command bus from `AppLifecycle`, or call a lifecycle orchestration method for lifecycle-owned operations
- call the corresponding command-bus method
- map errors and command-specific results to the frontend response

Tauri commands must not:

- mutate `ShowState` directly
- call `ShowStateHandle` mutation methods
- build `AppViewState`
- emit `app-status-changed`
- own business or safety validation

React updates app state only from `app-status-changed`. Command return values are command-specific results for control flow and user feedback, not app state snapshots.

`get_app_status` should be removed or changed so it no longer returns `AppViewState`; recovery state delivery comes from projector emission. If a command exists only to force a refresh, it should request or cause a source-owner event rather than directly returning app state.

Frontend startup uses an explicit readiness handshake. React must register its `app-status-changed` listener before invoking a readiness command such as `frontend_ready`. The backend does not emit the initial `AppViewState` or start discovery/auto-connect/runtime producers until that readiness command is received.

The startup sequence is directly orchestrated:

1. Create the app-lifetime `AppEventBus`.
2. Create `ShowStateHandle` and `AppLifecycle`.
3. Construct projector inputs and register projector/listeners.
4. Wait for React to register its listener and invoke `frontend_ready`.
5. Publish or apply initial show projection state.
6. Emit the initial disconnected `AppViewState` from the projector.
7. Start discovery, auto-connect, or other event-producing runtime work.
8. Enter normal event-driven runtime mode.

Constructors should not start I/O or emit important events. Event producers start only through explicit lifecycle/start methods after listeners are registered.

## Runtime Lifecycle Cutover

Move `RuntimeHandles` and generation logic out of `ShellState` and into `AppLifecycle`.

`AppLifecycle` owns an app-lifetime/show-capable `AppCommandBus` from construction. This bus routes show/app/session commands while disconnected and is not recreated on reconnect/disconnect. Runtime connect and disconnect operations install or clear only the runtime command targets and generation-scoped handles. Discovery, new/open show file, lockout, and other show-owned commands must not require a live LV1 runtime.

`AppLifecycle` should provide explicit operations for:

- beginning a connection attempt and allocating the next generation
- publishing that generation as the active runtime generation on the app-lifetime event bus
- installing connected runtime handles for the active generation
- rejecting stale runtime handles
- exposing the app command bus
- aborting the current runtime
- clearing runtime handles for a matching generation
- clearing reconnect/runtime state on disconnect

Disconnect cleanup remains generation-guarded. Stale tasks must not clear current runtime state or send commands after disconnect/reconnect. Show-owned disconnect metadata cleanup is driven by generated disconnect facts on the app-lifetime `AppEventBus`. User-requested disconnect may publish that generated disconnect fact from lifecycle after aborting runtime handles, using the captured active generation and reason, but lifecycle must not directly mutate show-owned connection metadata.

Generation is runtime lifecycle state. It guards runtime handle installation, stale task cleanup, LV1/fade command routing, reconnect/disconnect behavior, and runtime event projection. It is distinct from projector `state_version`.

## Show-Owned Connection Metadata

Connection UI metadata currently stored in shell state moves to `show/`:

- discovered LV1 systems
- connected LV1 identity
- pending LV1 identity
- reconnect UI metadata

Commands and lifecycle operations that change this metadata should publish `ShowEvent` with the updated show-owned projection payload.

`lv1/` still owns the live connection mirror. `show/` owns only app/UI metadata derived from discovery, connect intent, connected identity, and reconnect status.

Connection metadata changes should be modeled as show-owned commands or show-owned command helpers called through `AppCommandBus`. Lifecycle may coordinate runtime tasks and publish runtime lifecycle facts, but UI metadata updates should still flow through the show-owned runtime listener into `AppCommandBus`, then mutate private `ShowState` and publish `ShowEvent`.

`show/` is responsible for handling active-generation disconnect facts that affect show-owned connection metadata. The projector must not clear or rewrite those fields in response to LV1 events.

## Show-File Ownership

Show-file DTO import/export, validation, selected-scene restoration, file metadata, dirty tracking, and operational logs are handled through `show::commands`.

Native dialogs and physical file IO may remain adapter/infrastructure responsibilities, but command handlers own the state transaction around successful load/save/new operations.

Saving should be an explicit two-phase transaction:

1. Tauri/infrastructure asks `AppCommandBus` to export the current show file DTO for a proposed `saved_at` timestamp.
2. Tauri/infrastructure writes the file and backup successfully.
3. Tauri/infrastructure calls `AppCommandBus` to mark the show saved with `path` and `saved_at`.
4. The show command handler updates file metadata, marks the show clean, logs the save, and publishes `ShowEvent`.

The show must not be marked clean before physical IO succeeds.

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
- Add `ShowProjectionState` for events and projector cache updates.
- Change `ShowEvent` to carry the current show-owned projection payload.
- Move command logic from `ShellState` and Tauri command handlers into `show::commands`.
- Move runtime handles and generation logic into `AppLifecycle`.
- Remove `ShellState` and its tests or rewrite tests against `show::commands`, `AppCommandBus`, `AppLifecycle`, and projector cache.
- Update `AppCommandBus` to route all app/session commands and queries.
- Update projector cache to apply show state from events, not pull from show.
- Move `state_version` ownership into projector cache as projection sequence state.
- Construct the projector and subscriptions at app setup, then emit initial disconnected state only after `frontend_ready`.
- Use one app-lifetime `AppEventBus` shared by show, lifecycle/runtime actors, and projector.
- Add generation to runtime-originated event payloads, publish active runtime generation changes, and ignore stale-generation runtime events in projector and show-owned runtime listeners.
- Remove projector cache side effects for show-owned connection metadata on disconnect.
- Update React command handling so command returns are not applied as app state.
- Remove direct command/log emits of `app-status-changed`.

`ShowSnapshot` should not remain the name for source-of-truth state. During the cutover, either remove it or rename DTO usage to clarify intent, such as `ShowProjectionState` for UI projection and `ShowFileState` or existing show-file DTO names for persistence. If a DTO named `ShowSnapshot` remains temporarily, it must not be the source-of-truth type and should be targeted for removal in the same cutover.

The branch does not need to compile at every intermediate edit. The final commit must compile, pass tests, and preserve safety behavior.

## Guardrails

Verify these invariants through behavior tests, compiler visibility, final searches, and code review checkpoints. Do not add brittle static guard tests for these architecture rules:

- `ShellState` is removed.
- `ActiveCommandBus` is removed.
- `ShowState` fields are private.
- Tauri commands do not call `ShowStateHandle` mutation methods.
- Mutating Tauri commands do not return `AppViewState`.
- React does not apply command results as app state snapshots.
- Only projector code emits `app-status-changed`.
- Projector does not pull state from `show`; it applies show state delivered by `ShowEvent`.
- Show/app mutations publish `ShowEvent` carrying current show-owned projection payload.
- There is one app-lifetime `AppEventBus`; runtime connect paths do not create isolated buses for projector-visible events.
- Projector owns `state_version` and initial disconnected `AppViewState` emission after frontend readiness.
- Runtime-originated events carry generation and stale-generation runtime events do not affect projection or show-owned state.
- Active runtime generation changes are published on the app-lifetime event bus and cause projector/show runtime listeners to accept events from the new generation.
- Projector does not clear show-owned connection metadata in response to LV1 disconnect events.
- The backend does not start runtime event producers or emit initial app state before `frontend_ready`.
- Broadcast lag recovery for missed show projection events is intentionally deferred.
- Save commands do not mark the show clean before file IO succeeds.
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
npm --prefix ui run format:check
npm --prefix ui run lint
npm --prefix ui run typecheck
npm --prefix ui run test
npm --prefix ui run build
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

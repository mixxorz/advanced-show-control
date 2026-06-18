# Single Crate Command Projection Architecture Design

## Context

The current Rust architecture is split across two crates:

- `advanced-show-control` at the repository root, with core modules in `src/`.
- `advanced-show-control-tauri` in `src-tauri/`, with Tauri command, projection, logging, file, and shell state code.

The runtime is event-driven, but the shell layer has accumulated app behavior that should not live at the Tauri boundary. `src-tauri/src/commands.rs` currently performs Tauri plumbing, state mutation, recall validation, snapshot construction, and direct `app-status-changed` emission. React also applies both command-returned snapshots and `app-status-changed` snapshots.

This creates multiple UI state paths and duplicates business rules between Tauri and core modules.

## Decision

Move to a single Rust crate rooted at `src-tauri/`. Move the current root `src/` modules into `src-tauri/src/`, keep Tauri's project root and config in `src-tauri/`, and make module boundaries the real architecture boundaries.

The final Rust package name is `advanced-show-control`. The current CLI/probe binary must be preserved as a binary target in the unified crate.

The target architecture is:

```text
React command
  -> ui/ Tauri adapter
  -> AppCommandBus
  -> owning module validates/mutates
  -> owning module publishes AppEvent
  -> projector consumes AppEvent
  -> projector emits app-status-changed at 10 Hz
  -> React updates appState
```

Logging is the only exception:

```text
tracing
  -> logging channel
  -> projector log input
  -> projector emits app-status-changed at 10 Hz
```

There is no separate projection dirty notifier. The projector has exactly two input sources: `AppEventBus` and the logging channel.

## Physical Crate Layout

The first migration keeps the Tauri project root at `src-tauri/` because Tauri config, build script, capabilities, generated schemas, icons, and frontend paths already depend on that layout.

Initial target layout:

```text
src-tauri/
  Cargo.toml
  build.rs
  tauri.conf.json
  capabilities/
  gen/
  icons/
  src/
    main.rs
    bin/
      lv1-probe.rs
    runtime/
    lv1/
    fade/
    scene_recall/
    show/
    ui/
    logging/
    projector/
    connection/
    diagnostics.rs
    time.rs
```

Mechanical migration requirements:

- Move current root `src/*` into `src-tauri/src/*`.
- Preserve current root `src/main.rs` as a binary target at `src-tauri/src/bin/lv1-probe.rs` with binary command name `lv1-probe`.
- Absorb root `Cargo.toml` dependencies into `src-tauri/Cargo.toml`.
- Remove `advanced-show-control = { path = ".." }` from `src-tauri/Cargo.toml`.
- Rename the remaining Rust package to `advanced-show-control`.
- Replace `advanced_show_control::...` imports with `crate::...`.
- Update package selectors in tests, docs, and verification commands.
- Update `ui/src/types.ts` comments and docs that point to `src-tauri/src/app_state/view.rs` or the old two-crate layout.
- Keep `tauri.conf.json` frontend paths unchanged during this migration.
- Keep `src-tauri/build.rs`, capabilities, icons, and generated schemas in place.

The root `Cargo.toml` should stop defining a separate core package and remain as a workspace-only manifest. The workspace should contain the single Rust package in `src-tauri/` so root-level workspace commands continue to work.

The CLI/probe binary currently provides LV1 discovery, listen, monitor, rate-test, fade-test, Vegas, and pan-family smoke-test commands. It remains a supported developer/live-diagnostic tool after the crate merge.

## Module Pattern

Existing module file patterns should continue. When adding or expanding modules, prefer established names such as:

- `actor.rs`
- `commands.rs`
- `events.rs`
- `handle.rs`
- `state.rs`
- `types.rs`

Do not invent one-off file organization unless the module has a clear need not covered by the existing pattern.

## State Ownership

`ShellState` should be eliminated by the end state.

Initial ownership target:

```text
show/
  owns broad app/show/session state for now:
  lockout
  scene configs
  cued scene
  selected scene
  show file path/name/dirty/last saved
  discovered LV1 systems
  connected and pending LV1 identity metadata used by AppViewState
  reconnect UI metadata
  UI-requested scene recall use case
  show/session command result types
  ShowEvent definitions

lv1/
  owns live console mirror:
  connection status
  current scene
  scene list
  channels
  live parameter values
  LV1 actor lifecycle state

fade/
  owns fade execution state:
  idle/running/blocked
  active fade targets
  manual override
  abort/overlap/disconnect behavior

scene_recall/
  owns LV1-observed scene recall fade automation

logging/
  owns tracing setup and UI-log input channel
  does not use AppEventBus
  does not write ShowState
  does not emit app-status-changed

ui/
  owns Tauri adapter concerns:
  command registration
  dialogs
  app setup
  Tauri runtime setup
  frontend serialization boundary

projector/
  owns AppViewState assembly and app-status-changed emission
  owns projection cache for coalesced UI state
```

`show/` is intentionally broad at first. Later refactors may extract feature modules such as `scenes/`, `sessions/`, or `connection/` after the command/event/projection boundary is stable.

Connection state ownership is split as follows without modifying `lv1/` responsibilities:

- `lv1/` owns live console mirror state: transport connection status, current scene, scene list, channel topology, and live parameter values.
- `show/` owns app/UI connection metadata: discovered LV1 systems, connected LV1 identity, pending LV1 identity, and reconnect UI metadata.
- `projector/` combines both into `AppViewState`.

## Command Boundary

`AppCommandBus` is the central app command boundary. Tauri commands must route through it instead of directly mutating state.

Tauri commands should:

- Deserialize frontend arguments.
- Call the matching `AppCommandBus` method.
- Serialize the actual command-bus return value.
- Map command errors to Tauri errors.

Tauri commands must not:

- Call `ShowState` or app state mutation methods directly.
- Build `AppViewState` for mutating commands.
- Emit `app-status-changed`.
- Own business or safety validation.
- Return `AppViewState`.

Example target flow:

```text
ui::cue_scene(scene_id)
  -> AppCommandBus::cue_scene(scene_id)
  -> show module validates scene exists
  -> show state stores cue
  -> AppEvent::Show(ShowEvent::SceneCued { ... })
  -> CueSceneResult returned to Tauri/React
```

Command return values are command results, not UI state snapshots. React awaits them for success/failure behavior only. No Tauri command, including startup, connection, discovery, reconnect, save/load, or explicit refresh commands, should send `AppViewState` to the frontend. `app-status-changed` is the only frontend `AppViewState` delivery mechanism.

Cross-module calls that mutate or query `show/` state must go through `AppCommandBus`. Tauri `ui/` code and other modules should not call `show/` mutation/query handles directly except inside the command-bus implementation or show-owned command handlers.

## Event Boundary

`AppEventBus` is the only non-log state-change notification source for the projector.

Current event groups should be expanded from:

```rust
AppEvent::Lv1(...)
AppEvent::Fade(...)
AppEvent::SceneRecall(...)
```

to include show/app state changes, beginning with:

```rust
AppEvent::Show(ShowEvent)
```

Additional groups may be added later if a broad `ShowEvent` becomes too large, but the first refactor should keep state ownership broad in `show/`.

Every non-log mutation that affects `AppViewState` must publish an `AppEvent`. No event means no projection.

Modules that publish events should own an `AppEventBus` reference in their handle/state/service. `AppCommandBus` should not own or receive `AppEventBus`; its current unused event-bus constructor parameter should be removed.

Events are notifications/facts for projection and policy subscribers. For LV1 and fade projection, the current implementation already accumulates event payloads into projection state; the new projector cache should preserve that model rather than pulling LV1/fade snapshots on every tick. For show/app state, the projector should treat `ShowEvent` as notification that show-owned snapshot data changed and pull a current show snapshot when building `AppViewState`.

## Projection Boundary

The projector is the only backend code allowed to emit:

```text
app-status-changed
```

Projector inputs are exactly:

```text
1. AppEventBus
2. logging channel
```

There is no projection dirty notifier. Commands do not notify the projector directly. They publish domain events through their owning module.

The projector coalesces pending changes and emits at the existing 10 Hz cadence. `AppViewState` should remain the frontend contract during this refactor unless a field is explicitly removed in a later design.

The projector owns a projection cache. It continuously consumes `AppEventBus` events and UI log events, updates the cache, marks itself dirty, and emits the cached/coalesced `AppViewState` on the 10 Hz tick. The cache replaces `ShellState` as the projection accumulator.

Projection cache rules:

- LV1 events update cached LV1 projection fields, preserving the current `ShellState::apply_lv1_event_to_projection` model.
- Fade events update cached fade projection fields, preserving the current `ShellState::apply_fade_event_to_projection` model.
- Show events mark show projection data stale; the projector pulls the current show snapshot before emitting.
- Logging events append to projector-owned log cache.
- Runtime lifecycle events update cached generation/reconnect/discovery metadata as owned by `show/` or the lifecycle owner.

Target projection flow:

```text
AppEventBus event
  -> projector applies event to projection cache or marks show snapshot stale
  -> projector marks dirty
  -> 10 Hz tick pulls stale show snapshot data if needed and builds AppViewState from cache
  -> app.emit("app-status-changed", snapshot)
```

Target logging flow:

```text
tracing event
  -> UI log channel
  -> projector log input
  -> projector appends log entry to projector-owned log cache
  -> projector marks dirty
  -> 10 Hz tick builds AppViewState
```

## React Boundary

React app state should update only from `app-status-changed`.

Commands should not return `AppViewState`, and React should not call `applySnapshot` on command return values. Initial app state delivery must also use `app-status-changed`. The app lifecycle owner should start the projector before frontend command flows can depend on state, then publish or schedule an initial projection through the projector. Startup, reconnect, and recovery flows may request projection indirectly by causing the owning module to publish an `AppEvent`, but they must not return `AppViewState` through command responses.

## Recall Ownership

UI-requested scene recall belongs to `show/` for now because the user is asking the app to recall a stored show scene config.

Target flow:

```text
ui::recall_scene(scene_id)
  -> AppCommandBus::recall_scene_by_id(scene_id)
  -> show module handles recall request
```

The `show/` recall request handler owns:

- Lockout check.
- Stored scene config lookup.
- Requested scene id validation.
- Current LV1 state lookup through the command/runtime boundary.
- Connected check.
- Exact LV1 scene identity validation.
- LV1 recall command dispatch through the command boundary.
- Recall request outcome event publication.

LV1-observed scene recall fade automation remains in `scene_recall/`:

```text
LV1 scene changed
  -> AppEventBus
  -> SceneRecallFader
  -> decide whether to run app-managed fade
```

`SceneRecallFader` continues to own fresh LV1 state checks, scene-list edit suppression, fade-policy decisions, scoped target validation, stored target validation, and generation guards before starting fades.

## Logging Boundary

Logging stays out of `show/` and out of `AppEventBus` to avoid feedback loops.

The existing tracing UI sink should become a projector input, not a separate projector. It should not emit `app-status-changed` and should not publish `AppEvent`.

Internal logging/projection errors must not be re-enqueued into the UI log channel.

## Show File Ownership

Show-file DTOs, import/export mapping, and show-file validation should move into `show/` with the rest of show/session state. Tauri `ui/` code should own file dialogs and platform IO plumbing, then call `AppCommandBus`/`show/` commands with paths or file contents as appropriate.

Physical file writes, backup pruning, and dialog integration may remain adapter/infrastructure code during early phases, but the serialized show format and mapping between DTOs and `ShowState` belong in `show/` after the crate merge.

Show-file boundary:

- `show/` owns DTOs, schema version, import/export mapping, pruning, and validation against LV1 scene snapshots.
- `ui/` owns native file dialogs only.
- File-system read/write and backup pruning are infrastructure. They may live in a small file IO module, but Tauri command handlers should not own show-file mapping or validation.
- Calls into show-file import/export or show-state mutation must go through `AppCommandBus`.

## Runtime Lifecycle

Current Tauri code has an `ActiveCommandBus` wrapper that stores `Option<AppCommandBus>` so commands can find the current runtime command targets. This is not a React command export; it is a Tauri-side holder for the current command bus during connect/disconnect/reconnect.

The end state removes `ActiveCommandBus`. Its role is replaced by an explicit app lifecycle owner. Runtime lifecycle ownership should move out of `ShellState` and into that owner, which manages:

- current `AppCommandBus`
- LV1 actor handle/task
- fade engine handle/task
- scene recall fader task
- projector task
- generation value and stale-runtime cleanup

Tauri `ui/` commands should receive or access the app lifecycle owner, then call `AppCommandBus`. They should not manage runtime handles directly.

The lifecycle owner is responsible for creating, installing, exposing, and clearing the current `AppCommandBus`. It is also responsible for starting the projector early enough to emit the initial `AppViewState` through `app-status-changed`.

## Current Direct Emit Inventory

Current direct `app-status-changed` producers to remove or consolidate:

- `src-tauri/src/commands.rs::emit_snapshot`, which calls `app.emit("app-status-changed", ...)`.
- Direct `emit_snapshot(...)` calls from Tauri commands and runtime setup in `src-tauri/src/commands.rs`.
- The existing 10 Hz shell-state projector in `src-tauri/src/commands.rs`, which should be replaced by the new projector/cache owner rather than left as a second implementation.
- `src-tauri/src/logging.rs::ui_log_projector`, which appends logs and emits `app-status-changed` directly.

The end state has exactly one direct `app.emit("app-status-changed", ...)` call site: the projector.

## Build And Verification Matrix

Every migration step should leave the app in working order. A step is not complete until the smallest relevant verification passes.

Before and after the crate move, verify:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --workspace`
- `cargo build --workspace`
- CLI binary build and at least parser/test coverage for the preserved probe binary
- Tauri app build path, including `src-tauri/build.rs`, `tauri.conf.json`, capabilities, generated schemas, icons, and frontend dist path
- frontend `npm run typecheck` after command contract changes

The implementation plan should include a specific Tauri CLI verification command after researching the correct non-interactive command for this repository, such as `npm run tauri -- build` if appropriate.

## Pre-Migration Characterization

Before changing behavior, add or identify characterization coverage for:

- current direct `app-status-changed` emitters
- current projector 10 Hz cadence
- current log delivery behavior
- current command-return snapshot usage in React
- current UI-requested recall validation behavior
- current CLI/probe binary parser/build behavior

These tests may be updated or replaced during migration, but they should first make the existing behavior explicit so each step can be reviewed safely.

## Migration Phases

1. Add or identify pre-migration characterization coverage.
2. Move root `src/*` into `src-tauri/src/*`, preserve the CLI/probe as `src-tauri/src/bin/lv1-probe.rs`, rename the remaining package to `advanced-show-control`, and make the root `Cargo.toml` workspace-only.
3. Update imports, manifests, tests, docs, CI/package selectors, verification references, and frontend type comments after the crate move.
4. Add target module skeletons for `ui/`, `projector/`, `show/events.rs`, `show/commands.rs`, and the app lifecycle owner, following existing module file patterns.
5. Introduce the app lifecycle owner before major command routing. It owns runtime handles, generation, current command bus exposure, projector startup, and stale-runtime cleanup.
6. Move Tauri adapter code into `ui/` while preserving behavior. The adapter may still call existing code temporarily, but new business logic must not be added there.
7. Add `AppEvent::Show(ShowEvent)` and remove `AppEventBus` ownership from `AppCommandBus`; modules that publish events own event-bus references.
8. Route low-risk show/app commands through `AppCommandBus`, including cue, lockout, selected scene, duration, scope edits, and store scene config.
9. Move show-file DTOs, import/export mapping, pruning, and validation into `show/`.
10. Route show-file commands through `AppCommandBus`, using show-owned import/export/mapping and publishing `ShowEvent`.
11. Move UI-requested recall validation and dispatch into `show/`, reached through `AppCommandBus`.
12. Build the new projector cache. It accumulates LV1/fade events like current `ShellState`, pulls stale show snapshots after `ShowEvent`, owns log cache, and emits at 10 Hz.
13. Move logging into the projector input path so logging no longer emits `app-status-changed`.
14. Switch to projector-only `app-status-changed` emission by removing direct command/log emits and replacing the old shell-state projector.
15. Update React so commands return command results and app state updates only from `app-status-changed`.
16. Eliminate `ShellState` after projector cache and module-owned state replace all of its responsibilities.
17. Remove `ActiveCommandBus` after the app lifecycle owner fully replaces its role.
18. Add final docs and guardrails for the enforced architecture.

## Guardrails

Add tests or static checks for these invariants:

- Only projector code emits `app-status-changed`.
- Mutating Tauri commands do not return `AppViewState`.
- React does not apply mutating command returns as app state.
- Show/app mutations publish `AppEvent::Show`.
- Logs reach the UI only through the projector logging input.
- UI-requested recall validation is not in `ui/` or Tauri command adapters.
- `ShellState` is removed by the end state.
- `ActiveCommandBus` is removed.
- The CLI/probe binary remains buildable.
- No command returns `AppViewState` to the frontend.

## Non-Goals

- No frontend `AppViewState` schema redesign in this refactor.
- No immediate extraction of `scenes/`, `sessions/`, or `connection/` modules from `show/`.
- No change to LV1 protocol behavior.
- No changes to `lv1/` ownership for connection status or live console mirror responsibilities.
- No weakening of lockout, exact scene identity, generation guards, disconnect behavior, manual override, abort, or overlap safety behavior.
- No routing logs through `AppEventBus`.

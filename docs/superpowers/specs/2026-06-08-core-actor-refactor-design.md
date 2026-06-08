# Core Actor Refactor Design

## Goal

Refactor the backend around independent core modules that communicate through `AppEventBus` and `AppCommandBus`. The Tauri crate should become a thin desktop adapter. It should not own non-UI app behavior, scene recall policy, or app-managed show state.

This refactor intentionally breaks internal Rust module paths. No backwards-compatibility shims, stale re-exports, duplicate module paths, or dead code should remain after the refactor.

## Architecture Direction

The app should remain actor/module-oriented. There should be no central `AppRuntime` authority that owns all state and decision-making.

Core modules should own their own state and communicate through bus interfaces:

- `Lv1Actor` owns LV1 TCP connection state and the LV1 mirror.
- `FadeEngine` owns active fade timing and overlap behavior.
- `ShowState` owns app-managed show data only.
- `SceneRecallFader` owns scene recall automation policy.
- `Tauri Shell` adapts UI/native commands to bus commands and projects bus events to React.

The buses define module boundaries:

- `AppEventBus` broadcasts facts that happened.
- `AppCommandBus` routes addressed commands and queries to module handles.

Modules must not reach directly into another module's state. Direct shared-state access such as `SceneRecallFader -> ShellState` should be removed.

## State Ownership

`src/` owns behavior and safety state:

- LV1 connection mirror and live LV1 state.
- Active fade execution state.
- App-managed scene fade configs.
- Scoped channel refs.
- Stored target fader values.
- Fade duration and curve config.
- Lockout.
- Scene recall trigger state and recall policy state.

`src-tauri/` owns desktop adapter state:

- Tauri command handlers.
- Window event emission.
- File dialogs.
- Native filesystem path concerns.
- Show-file path, dirty marker, and last-saved timestamp unless later moved behind a core persistence actor.
- Connection preferences.
- React `AppViewState` projection glue.

`ui/` owns only browser-local interaction state:

- Active tab.
- Connection panel visibility.
- Transient command errors.
- Other local view affordances.

## Module Responsibilities

### LV1

`Lv1Actor` exposes commands:

- `get_lv1_state`
- `set_gain`

It publishes events:

- connected
- disconnected
- scene changed
- scene list changed
- channel topology changed
- fader changed
- mute changed

### Fade

`FadeEngine` exposes commands:

- `start_fade`
- `abort_all_fades`

It publishes fade lifecycle and channel events.

There must be no `finish_now` command. Same-scene recall behavior is handled through fade overlap behavior when a valid recall fade starts.

### Show

`ShowState` owns data, not scene recall policy.

It exposes commands and queries:

- `get_show_snapshot`
- `get_scene_config`
- `get_lockout`
- `set_lockout`
- `set_scene_duration`
- `set_channel_scoped`
- `set_all_channels_scoped`
- `store_scene_config`
- `load_show_data`
- `export_show_data`

It publishes show-state facts such as show-state changes, scene-config changes, and lockout changes.

### Scene Recall

`SceneRecallFader` is a core module. It is not generic automation. Do not place it under an `automation` module, because future rule-based automation should have its own distinct module.

`SceneRecallFader` exposes no commands initially. It is event-driven.

It subscribes to LV1 scene-change facts, uses command-bus queries to fetch LV1 and show data, makes the recall decision itself, and commands `FadeEngine` to start a fade when validation succeeds.

It owns:

- recall arming delay
- same-scene repeat suppression
- duration-zero skip suppression, if suppression remains required
- exact scene validation
- lockout block decision
- live topology validation
- stored fader target validation
- fade request construction

It publishes scene-recall facts such as skipped, blocked, ready, and start-requested.

## File Structure

Core crate target:

```text
src/
  lib.rs

  runtime/
    mod.rs
    commands.rs
    events.rs

  lv1/
    mod.rs
    actor.rs
    handle.rs
    commands.rs
    events.rs
    state.rs
    types.rs
    messages.rs
    parsers.rs
    tcp.rs
    discovery.rs
    probe.rs

  fade/
    mod.rs
    actor.rs
    handle.rs
    commands.rs
    events.rs
    state.rs
    types.rs
    tick.rs
    curve.rs
    fader_law.rs

  show/
    mod.rs
    actor.rs
    handle.rs
    commands.rs
    events.rs
    state.rs
    types.rs
    capture.rs

  scene_recall/
    mod.rs
    actor.rs
    events.rs
    state.rs
    policy.rs

  osc.rs
  vegas.rs
  main.rs
```

Tauri crate target:

```text
src-tauri/src/
  main.rs
  commands.rs
  show_file.rs
  connection_state.rs
  connection_preferences.rs

  app_state/
    mod.rs
    shell.rs
    view.rs
    projection.rs
    logs.rs
    show_file_mapping.rs
```

Remove or absorb these files:

- `src-tauri/src/scene_recall_fader.rs`
- `src-tauri/src/app_state/scene_recall.rs`
- `src-tauri/src/app_state/capture.rs`; move domain behavior to `src/show/capture.rs` and put any remaining Tauri command wrappers in `src-tauri/src/commands.rs` or another clearly named adapter file
- old module names replaced by the new standard, such as `lv1/model.rs` and `fade/engine.rs`

## File Naming Standard

Command-target actor modules use this shape:

```text
actor.rs      spawn function and async task loop
handle.rs     cloneable handle used by AppCommandBus
commands.rs   command enum and reply payloads
events.rs     facts published to AppEventBus
state.rs      private mutable actor state
types.rs      public data/config types
```

Event-only actors use only the files they need:

```text
actor.rs
events.rs
state.rs
policy.rs, when pure decision logic is useful
```

Do not create empty command or handle modules unless the actor actually exposes commands.

## Bus Contracts

`AppCommandBus` should route to command-capable module handles:

- `Lv1ActorHandle`
- `FadeEngineHandle`
- `ShowStateHandle`

`SceneRecallFader` is not an `AppCommandBus` target unless a real external command for it is introduced.

`AppEvent` should wrap module event types:

- `Lv1(Lv1Event)`
- `Fade(FadeEvent)`
- `Show(ShowEvent)`
- `SceneRecall(SceneRecallEvent)`
- `CommandFailed { command, message }`

The old generic automation refresh event should be replaced by scene-recall-specific events for scene recall behavior. Future rule-based automation should get its own event type later.

## Tauri/UI Projection

React-facing behavior should remain stable unless the implementation plan explicitly changes it:

- Keep the `AppViewState` contract stable.
- Keep the `app-status-changed` event stable.
- Tauri commands should still return snapshots where the UI expects snapshots.

The source of the snapshot changes: Tauri should project core module snapshots plus desktop shell metadata into `AppViewState`.

## Testing Strategy

Move tests with the behavior they cover:

- Scene recall decision tests move to `src/scene_recall/policy.rs` tests.
- Scene recall trigger-gate tests move to `src/scene_recall/state.rs` tests.
- Scene recall actor tests move to `src/scene_recall/actor.rs` tests where they can run without Tauri.
- Show config mutation tests move to `src/show/` tests.
- LV1 and fade tests should be updated to the new file structure, not left behind under old module names.
- Tauri tests should remain only for Tauri command mapping, native persistence, and UI projection.

Safety behavior must stay covered:

- lockout blocks scene recall fades
- exact scene identity validation
- stale generation protections or equivalent lifecycle protections
- unavailable LV1 state blocks fades
- missing live topology blocks fades
- missing stored fader target blocks fades
- blocked or skipped recalls do not abort active fades
- valid recalls start fades through `FadeEngine`
- same-scene behavior remains owned by fade overlap handling

## Cleanup Requirements

This refactor should leave no compatibility layer behind.

Required cleanup:

- Remove old files after their contents move.
- Remove stale module declarations.
- Remove stale imports.
- Remove re-exports that exist only to preserve old paths.
- Remove dead code and unused helpers exposed only for the old structure.
- Rename modules fully instead of keeping aliases such as `lv1::model` or `fade::engine` for compatibility.
- Update documentation to match the new architecture, especially `docs/architecture.md`.

Internal callers should use the new module paths directly.

## Documentation Requirements

Update `docs/architecture.md` as part of the refactor. The architecture document should describe the final actor/module boundaries, not the pre-refactor `ShellState`-centered design.

It should cover:

- `AppEventBus` as the facts boundary.
- `AppCommandBus` as the command/query boundary.
- `Lv1Actor`, `FadeEngine`, `ShowState`, and `SceneRecallFader` as independent modules.
- `SceneRecallFader` owning scene recall policy and decision-making.
- `ShowState` owning show data only.
- Tauri as a thin desktop adapter and UI projection layer.
- The final file/module structure.
- The rule that no module directly reaches into another module's state.
- The removal of generic `AutomationEvent` for scene recall behavior in favor of scene-recall-specific events.

## Non-Goals

- Do not introduce a central `AppRuntime` authority.
- Do not make `ShowState` decide scene recall policy.
- Do not add a generic `automation` module for `SceneRecallFader`.
- Do not preserve old internal Rust module paths.
- Do not add `finish_now` back to `FadeEngine`.

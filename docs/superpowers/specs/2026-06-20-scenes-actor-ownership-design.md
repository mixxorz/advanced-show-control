# Scenes Actor Ownership Design

## Goal

Replace the current `scene_recall` module with a `scenes` actor/module that owns all app-managed scene behavior. This is a single complete move, not a staged compatibility split.

LV1 remains the source of truth for raw console scene state. The new `scenes` actor owns the app's scene overlay: managed scene configs, fader scope, stored targets, selected/cued scene state, LV1 scene-list reconciliation against app-managed scenes, and scene recall automation.

## Non-Goals

- Do not move lockout out of `show`.
- Do not move show-file path/name/dirty/save metadata out of `show`.
- Do not add temporary wrappers, re-export bridges, or legacy compatibility APIs.
- Do not keep duplicate scene state in `show` and `scenes`.
- Do not introduce a coordinator actor or trait-based command bus.

## Architecture

Rename `src-tauri/src/scene_recall/` to `src-tauri/src/scenes/` and evolve that module in place. The renamed module becomes the owner of scene recall policy plus the app-managed scene state currently held by `show`.

The public module surface remains strict:

- `scenes::build_actor(...) -> (ScenesActorHandle, ScenesActorTask, ScenesActorPeers)`
- `ScenesActorHandle` is a dumb mailbox sender only.
- `ScenesCommand` is the only command enum callers send to the actor.
- `ScenesActorTask` owns the task and actor runtime.
- `ScenesActorPeers` owns direct peer wiring before spawn.
- Submodules are private unless a root-level facade item is intentionally exposed.

There is no `scene_recall` compatibility module after the move. All call sites switch to `scenes` in the same change.

## Ownership

`scenes` owns:

- `SceneConfig`, `SceneScopeToggles`, `ChannelConfig`, `ChannelRef`, `scene_id(...)`, and `parse_scene_id(...)`.
- Managed scene configs.
- Selected scene id.
- Cued scene id.
- Scene config lookup.
- Scene duration changes.
- Scene fader and pan scope toggles.
- Per-channel and all-channel scope changes.
- Scene selection and cueing.
- Capturing/storing scene config from the current LV1 channel snapshot.
- Reconciling app-managed scene configs with the LV1 scene list.
- Scene recall request validation and recall automation.
- Recall policy state that currently lives under `scene_recall`.
- Scene projection state and scene-related events consumed by the projector.

`show` owns:

- Lockout state.
- Show file path, display name, dirty state, save timestamps, and file IO orchestration.
- LV1 discovery and connection metadata currently owned by show.
- Persistence workflows that read or write show files.

`lv1` owns:

- The raw LV1 scene mirror.
- Current LV1 scene observation.
- Raw LV1 scene list entries.
- LV1 scene recall TCP command execution.

`fade` owns:

- Active fade execution.
- Fade timing and target writes.
- Fade abort/manual override/disconnect safety.
- Fade scene identity during active fades.

## Data Flow

UI scene commands become thin adapters that construct `ScenesCommand` messages and send them to `ScenesActorHandle`. UI show-file commands still send `ShowCommand` messages to `ShowStateHandle`.

For scene recall, `ScenesCommand::RecallScene` validates the request using scene-owned config state, fresh LV1 state, show-owned lockout state, and existing generation safety. If validation passes, `scenes` sends the LV1 recall and fade command through its directly wired peers. Blocked, skipped, or disabled recalls remain visible through events/logs and must not abort an existing fade.

For scene list changes, `scenes` subscribes to the app event bus and consumes LV1 scene-list facts. It reconciles managed scene configs from those facts. LV1 still owns the raw mirror; `scenes` owns only how app-managed scenes track LV1 scene identity changes.

For show-file load and save, `show` remains the file IO owner. On load, `show` parses the persisted document and sends the scene payload to `scenes` as a command. On save, `show` requests a scene export snapshot from `scenes`, combines it with show-owned metadata, and writes the show file. This is normal actor communication, not a compatibility bridge, and `show` does not retain duplicate scene state.

## Projection

The projector consumes a new scenes projection/event type in addition to the existing show, LV1, fade, and lifecycle facts.

Show projection no longer contains scene config, selected scene, or cued scene fields. Scenes projection provides those fields. `AppViewState` continues to expose the same frontend-facing scene data, but the source becomes the `scenes` actor projection.

## Runtime Wiring

Lifecycle builds and wires actors before spawn:

- Build LV1.
- Build fade.
- Build show.
- Build scenes, using the renamed former `scene_recall` actor as the base.
- Wire direct peers through `ScenesActorPeers`, `FadeEnginePeers`, `ShowActorPeers`, and existing runtime generation state.
- Install runtime handles.
- Spawn tasks.

Peer wiring remains direct actor-owned state. There are no mailbox commands for peer installation and no peer setters on runtime handles.

## Error Handling And Safety

Safety behavior must be preserved:

- Do not bypass lockout checks. Lockout stays show-owned, but recall validation must still consult it.
- Do not bypass exact scene identity validation.
- Do not bypass generation guards.
- Do not send fader commands when LV1 state is unavailable, disconnected, stale, or unsafe.
- Validate scene recall before aborting an existing fade.
- Blocked, skipped, or disabled recalls must not abort an existing fade.
- Use fresh LV1 state for recall automation where event subscriber ordering could otherwise create stale decisions.
- Make safety blocks visible through events, logs, or UI state.
- Preserve manual override, abort, overlap/same-scene, and disconnect safety behavior.

Command failures that cross the UI boundary continue to map through the runtime command error model. Internal actor errors should remain explicit and typed where the current module patterns support it.

## Testing

Use compile errors as the first RED signal for the big move, then add or move behavior tests with the ownership changes.

Required test coverage:

- Scene command handling after the module rename.
- Scene config mutation and lookup under `scenes`.
- Scene capture from LV1 channel snapshots under `scenes`.
- Scene-list reconciliation under `scenes`.
- Show-file load/import sends scene payload to `scenes` and does not leave duplicate scene state in `show`.
- Show-file save/export requests scene snapshot from `scenes`.
- Projector merges scenes projection into `AppViewState`.
- Recall safety behavior remains unchanged after moving recall into `scenes`.
- Existing fade safety and LV1 generation tests continue to pass.

Final verification for the implementation must include:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
```

# Scenes State Refactor Design

## Goal

Move app-managed scene configuration state out of `show` and into `scenes`, while preserving existing frontend command names and safety behavior.

`scenes` will own scene configuration, scene selection/cueing, scene edit commands, scene alignment, capture/store behavior, and recall validation inputs. `show` will own session file metadata, lockout, discovery/connection metadata, reconnect metadata, and persistence orchestration.

## Ownership Boundary

`scenes` becomes the owner of all app-managed scene state:

- Move `SceneConfig`, `ChannelConfig`, `ChannelRef`, `SceneScopeToggles`, and the scene snapshot/document type from `show` to `scenes`.
- Expand `ScenesState` so it owns scene configs, cued scene ID, selected scene ID, recall gate state, and scene-list edit suppression state.
- Move scene alignment from `show::scene_alignment` to `scenes::scene_alignment`.
- Move capture/store behavior from `show::capture` to `scenes`.
- Import scene-domain public types from `crate::scenes`, not `crate::show`.

`show` remains responsible for:

- Lockout.
- Session file path, display name, dirty flag, and last saved timestamp.
- LV1 discovery/connection metadata and reconnect state.
- Reading, writing, importing, and exporting `.ascs` files in coordination with `scenes`.

Update `docs/architecture.md` so `show` no longer claims ownership of application-managed scene configuration state.

## Commands And Actor Flow

Move scene edit/read commands from `ShowCommand` to `ScenesCommand`:

- `GetSceneDocument` or equivalent scene snapshot command.
- `GetSceneConfig`.
- `InitialProjectionState` for scenes.
- `SetSceneDuration`.
- `SetSceneScopeFadersEnabled`.
- `SetSceneScopePanEnabled`.
- `LinkSceneConfig`.
- `DeleteSceneConfig`.
- `SetChannelScoped`.
- `SetAllChannelsScoped`.
- `CueScene`.
- `SelectSceneConfig`.
- `StoreSceneConfigFromCurrentLv1`.
- Test-only replacement/clear commands only if actor tests still need explicit scene-state seams.

Keep external Tauri command function names stable. The adapters for scene-related commands will route to `ScenesHandle` instead of `ShowStateHandle`, so the frontend does not need a matching command-name migration.

`ScenesPeers` should no longer carry `ShowStateHandle` for normal recall validation. `scenes` should validate explicit and observed recalls against its own state, fetch fresh LV1 state where required, and then send LV1/fade commands through its LV1 and fade peers.

## Projection And Events

Split projection state by ownership:

- Keep `ShowProjectionState` for show-owned metadata only.
- Add `ScenesProjectionState` containing scene configs, cued scene ID, and selected scene ID.
- Extend `ScenesEvent` with a state-change event carrying `ScenesProjectionState` and a reason/change kind.
- Keep recall-only `ScenesEvent` variants such as skipped, blocked, ready, and start-requested.
- Seed the projector with both initial show state and initial scenes state.
- Apply `ShowEvent::StateChanged` for session/connection metadata.
- Apply `ScenesEvent::StateChanged` for scene UI data.
- Keep `AppViewState` externally unchanged so the frontend still receives the same fields through `app-status-changed`.

`ShowProjectionReason::ShowState` should be removed or renamed if no remaining show-owned state uses that reason after scene state is split out.

## Persistence And Dirty State

Show-file DTOs can remain under `show::show_file` because they describe session-file format and I/O, but their conversions should use scene-domain types from `scenes`.

Persistence coordination:

- New session: `show` fetches current LV1 state, asks `scenes` to reset and align scene state, resets file metadata, and marks the session clean.
- Save: `show` asks `scenes` for the current scene document and exports it with show-owned lockout/file metadata.
- Open: `show` reads/imports the file, asks `scenes` to replace and align scene state, updates file metadata, and marks dirty if import generated IDs, removed data, or alignment changed.
- Load/reconciliation warnings remain visible through tracing logs.

Dirty-state ownership stays in `show`:

- `scenes` publishes scene state-change facts.
- `show` subscribes to persisted scene-edit changes and marks the session dirty.
- File-driven scene replacement/reset commands should identify themselves so `show` does not incorrectly dirty a clean open/new session unless the import result requires it.

## Cleanup Scope

Delete or stop exposing code that becomes obsolete because of the boundary move:

- Remove `ShowCommand` variants that only read or mutate scene state.
- Remove `ShowState` fields and methods for scene configs, selected scene ID, and cued scene ID.
- Remove `show::capture` after capture/store moves into `scenes`.
- Remove or move `show::scene_alignment`; no `show` wrapper should remain.
- Remove `show` re-exports for scene-domain types once callers use `scenes`.
- Remove `ShowProjectionState` fields for scene configs, cued scene ID, and selected scene ID.
- Remove `ScenesPeers.show` and any scene recall path that fetches scene configs through `ShowCommand`.
- Remove tests that only verify scene behavior through `ShowCommand`/`ShowStateHandle`; replace with `ScenesCommand`/`ScenesHandle` tests.

Do not delete unrelated unused show commands in this refactor. `ShowCommand::GetLockout` and `ShowCommand::SetDiscoveredLv1Systems` are unrelated cleanup candidates and are tracked in `docs/roadmap.md`.

Do not delete:

- Tauri command function names used by the frontend.
- `.ascs` schema fields for scene config or cued scene data.
- Lockout ownership in `show`.
- Existing recall policy safety behavior.

## Tests

Use the project's allowed Rust test styles.

Pure unit tests:

- Move scene type serialization tests to `scenes::types`.
- Move alignment tests to `scenes::scene_alignment`.
- Move recall validation tests from `show::commands` to `scenes`.
- Keep show-file import/export tests in `show::show_file`, updated to import scene types from `scenes`.

Actor tests:

- Move scene edit command tests from `show` actor/handle tests to `scenes` actor tests.
- Exercise scene edits through `ScenesCommand` and `ScenesHandle`.
- Cover `ScenesEvent::StateChanged` publication for scene edits.
- Cover `show` marking file dirty when persisted scene-state changes arrive.
- Cover `show` not marking dirty for file-driven reset/replace events unless import/reconciliation requires dirty state.

Projector tests:

- Update projector seeding to include initial scenes state.
- Update event tests so show events update show-owned fields and scenes events update scene-owned fields.

## Verification

Inner loop:

```bash
cargo nextest run -p advanced-show-control scenes show projector
```

Before completion:

```bash
make rust-fmt
make rust-lint
make rust-test
```

If Rust command result shape changes affect TypeScript usage, run:

```bash
make ui-typecheck
```

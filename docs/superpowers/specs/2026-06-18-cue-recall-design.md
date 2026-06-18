# Cue And Recall Design

## Goal

Make the existing Cue, Recall, and Go controls functional.

The app should store one cued scene in app show state. Pressing Go recalls the currently cued scene and leaves that cue set. Auto-next and cue lists remain future work.

## Scope

In scope:

- Store a single `cuedSceneId` in backend show state and project it to the frontend.
- Add backend commands for cueing and recalling scenes.
- Wire the existing frontend command surface to the new Tauri commands.
- Recall an LV1 scene by sending `/Set/CurSceneIndex` for the selected scene index.
- Keep existing `SceneRecallFader` behavior as the only path that starts app-managed fades after LV1 confirms the scene recall.
- Log cue changes, recall requests, successful recall command sends, blocked recalls, and command failures using established request/fact logging levels without duplicating events.

Out of scope:

- Auto-next.
- Cue lists.
- Clearing the cue after Go.
- Changing scene matching rules for app-managed fades.
- Replacing LV1 scene ownership or bypassing LV1 as the source of truth.

## Architecture

Use the existing command architecture:

```text
Frontend -> Tauri command -> ShellState/AppCommandBus -> Lv1Actor -> LV1 TCP
```

`ShowState` owns the stored cue because it is app show data. `ShellState` owns command projection and validation that depends on current runtime state. `AppCommandBus` routes recall requests to the current LV1 target. `Lv1Actor` owns the actual OSC write.

Scene recall fades are not started directly by the UI command. The UI command asks LV1 to recall a scene. LV1 then emits current-scene notifications, and `SceneRecallFader` applies the existing safety checks before starting any app-managed fade.

## Data Model

Add `cued_scene_id: Option<String>` to `ShowState` and `ShowSnapshot`.

Projection maps this field to `AppViewState.cuedSceneId` so existing frontend components can display the cued scene and enable Go.

Cue storage changes mark the show dirty when the stored cue actually changes. Loading a show file restores the stored cue only if it is present in the serialized show snapshot. New show files start with no cue.

## Commands

Add `cue_scene(scene_id: String) -> AppViewState`.

Behavior:

- Validate that `scene_id` exists in `ShowState.scene_configs`.
- Do not block on lockout, because cueing only changes app show state and sends no command to LV1.
- Store it as `cued_scene_id`.
- Emit a fresh snapshot.
- Log the request at debug level and the resulting `scene_cued` fact at info level with scene id, index, and name.
- Return an error and log `scene_cue_blocked` when the scene id is unknown.

Add `recall_scene(scene_id: String) -> AppViewState`.

Behavior:

- Validate that lockout is disabled.
- Validate that LV1 is connected and fresh state is available.
- Validate that `scene_id` exists in show state.
- Validate that the scene id resolves to the same index/name in the current LV1 scene list.
- Send `/Set/CurSceneIndex` with the resolved scene index through `AppCommandBus` and `Lv1Actor`.
- Emit a fresh snapshot after the command is accepted.
- Leave `cued_scene_id` unchanged.

## Safety Rules

App-initiated recall must not bypass existing safety behavior.

- Lockout blocks recall before any LV1 command is sent.
- Lockout does not block cueing, because cueing only stores app state.
- Missing LV1 state, disconnected LV1, or unavailable command targets block recall.
- Scene id mismatch blocks recall.
- The command does not abort or replace active fades directly.
- Existing `SceneRecallFader` validation remains responsible for deciding whether the confirmed LV1 recall should start, skip, or block app-managed fades.
- Existing generation guards remain at the command-bus boundary so stale runtime handles cannot write after disconnect or reconnect.

## LV1 Protocol

Add an LV1 command variant and handle method for scene recall:

```text
/Set/CurSceneIndex i:<scene-index>
```

The actor sends this on the current writer and replies with the existing LV1 command error style. Tests should assert the encoded OSC address and integer argument where practical.

## Logging

Use `tracing` so messages flow to diagnostic logs and frontend-facing info logs according to the existing logging pipeline. Follow existing conventions: user requests are debug-level, resulting operational facts are info-level, and blocked safety outcomes are warning-level. Do not add duplicate logs for the same fact at multiple layers.

Log at debug level:

- `scene_cue_requested` when the user requests a cue change.
- `scene_recall_requested` when the user requests recall.

Log at info level:

- `scene_cued` when a scene is cued.
- `scene_recall_command_sent` when `/Set/CurSceneIndex` is accepted by the LV1 command path.

Log at warning level:

- `scene_cue_blocked` for unknown scene ids.
- `scene_recall_blocked` for lockout, missing LV1 state, disconnected LV1, unavailable command target, or scene mismatch.
- `scene_recall_command_failed` when the LV1 command path returns an error.

Logs should include scene id when available, plus scene index and scene name when resolved. Blocked logs should include a short reason suitable for UI troubleshooting.

## Frontend

Wire existing commands:

- `cueScene(sceneId)` invokes `cue_scene`.
- `recallScene(sceneId)` invokes `recall_scene`.
- The Go button continues to call `recallScene(appState.cuedSceneId)`.

No visual redesign is required. Existing status bar and scene-row cue indicators should work from `AppViewState.cuedSceneId` once projection includes the field.

## Testing

Rust tests:

- `ShowState` stores, projects, and replaces `cued_scene_id`.
- Cueing an unknown scene returns an error and does not change state.
- `AppViewState` includes the cued scene id.
- Recall is blocked by lockout.
- Recall is blocked when LV1 is unavailable or disconnected.
- Recall is blocked when the scene id does not match the current LV1 scene list.
- Recall sends the expected `/Set/CurSceneIndex` command for a valid scene.
- Recall leaves `cued_scene_id` unchanged.

Frontend tests:

- Existing status bar and story coverage should remain valid.
- Add or update a wiring test only if command construction changes in `AppRuntime` require coverage.

Verification:

- Run targeted Rust tests for show state, Tauri commands, and LV1 command handling.
- Run `cargo fmt --all -- --check`.
- Run broader Rust checks if implementation touches shared command or actor code beyond the targeted path.

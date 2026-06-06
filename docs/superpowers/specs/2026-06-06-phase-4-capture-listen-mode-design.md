# Phase 4 Capture Engine And Listen Mode Design

## Purpose

Phase 4 adds the preferred scene fade setup workflow to the existing Tauri shell. The app shows the LV1 scene list, lets the engineer select a scene to edit, and records fader target values while Listen Mode is active.

This phase is in-memory only. Disk persistence, durable scene matching, rename/reorder handling, project files, mismatch warnings, and remapping belong to Phase 5.

## Naming

Avoid the word "snapshot" for new Phase 4 concepts because sound console users may read it as saved scene terminology.

Use these names for new or renamed app-facing concepts:

- `AppViewState`: serializable state sent from Rust to React.
- `SceneFadeConfig`: editable fade setup for one LV1 scene.
- `FadeTarget`: one fader destination value for a scene fade.
- `listenModeActive`: whether fader notifications are currently being written into the selected scene config.

Existing internal names such as `Lv1StateSnapshot` may stay unchanged in Phase 4.

## State Model

`ShellState` remains the Rust-owned mutable state holder. React renders the serialized `AppViewState` and sends commands; React does not own capture state.

Phase 4 adds these app-facing fields:

```ts
type AppViewState = {
  connection: ConnectionState;
  currentScene: SceneSummary | null;
  sceneFadeConfigs: SceneFadeConfig[];
  selectedSceneId: string | null;
  listenModeActive: boolean;
  fadeState: FadeState;
  lockout: boolean;
  logs: AppLogEntry[];
  lastEventAt: string | null;
};

type SceneFadeConfig = {
  sceneId: string;
  sceneIndex: number;
  sceneName: string;
  fadeEnabled: boolean;
  fadeTargets: FadeTarget[];
};

type FadeTarget = {
  group: number;
  channel: number;
  targetDb: number;
  enabled: boolean;
  updatedAt: string;
};
```

`sceneId` is temporary for Phase 4 and derived from scene index plus scene name. This is acceptable because Phase 4 does not persist project files. Phase 5 must replace or supplement this with persistence-aware scene tracking for renamed and reordered scenes.

No starting fader value is stored. Recall fades will start from the current live LV1 value and move to the stored `targetDb`.

No channel name is stored on `FadeTarget`. The UI derives the current display name from the mirrored LV1 channel list by `group` and `channel`, so channel renames are reflected immediately.

No capture threshold is used. While Listen Mode is active, every fader gain notification writes the latest value into the selected scene's target list.

## Scene List Reconciliation

When the LV1 scene list changes, `ShellState` reconciles `sceneFadeConfigs` with the current scene list:

- Existing configs are matched by the temporary Phase 4 `sceneId` derived from index and name.
- Matching configs keep `fadeEnabled` and `fadeTargets`.
- New LV1 scenes get a default config with `fadeEnabled: false` and an empty `fadeTargets` list.
- Scenes no longer present in the LV1 scene list are removed from Phase 4 in-memory state.
- If the selected scene disappears, selection moves to the first available scene, or `null` if there are no scenes.
- If Listen Mode is active and the selected scene disappears, Listen Mode turns off and a warning is logged.

Known Phase 4 limitation: renaming or reordering a scene can cause the derived `sceneId` to change and may lose that scene's in-memory config. Phase 5 must address this with project-file validation, mismatch warnings, remap behavior, and safer matching.

## Listen Mode Behavior

Listen Mode is a direct-write mode for the selected scene config.

Workflow:

1. The engineer selects a scene from the app's LV1 scene list.
2. The engineer turns Listen Mode on.
3. The app locks scene selection while Listen Mode is active.
4. The engineer moves faders in LV1.
5. Each fader gain notification writes or updates a `FadeTarget` in the selected `SceneFadeConfig`.
6. The engineer may remove faders while Listen Mode is still active.
7. If a removed fader moves again while Listen Mode is active, it is captured again.
8. The engineer turns Listen Mode off.

The current live LV1 scene does not affect Listen Mode. The selected scene in the app is the edit target, even if LV1 is currently on another scene.

Listen Mode never sends commands to LV1. It only records fader notifications already coming from the LV1 mirror.

When a fader event arrives while Listen Mode is active:

- Find the selected scene config.
- Find the channel in the current LV1 mirrored channel list by group/channel so the event is known and display names can be derived.
- Insert a new `FadeTarget` if one does not exist for the event's group/channel.
- Update the existing target if it already exists.
- Set `targetDb` to the event value.
- Set `updatedAt` to the current app timestamp.
- Default new targets to `enabled: true`.

## UI Design

The existing `Scene` tab becomes a split list/detail layout.

Left side:

- Shows all scene fade configs from the current LV1 scene list.
- Each row shows scene index, scene name, fade enabled state, and target count.
- Clicking a row selects that scene for editing.
- While Listen Mode is active, scene rows are disabled so selection cannot change.

Right side:

- Shows the selected scene index and name.
- Shows a `Fade Enabled` toggle for the selected scene.
- Shows a `Listen Mode` toggle.
- Shows the selected scene's fader target table.

Target table columns:

- Include/enabled checkbox.
- Group.
- Channel.
- Current channel name, derived from the LV1 mirror.
- Target dB.
- Updated at.
- Remove action.

The target table remains editable while Listen Mode is active. Removing a target deletes it immediately. If the same fader moves again during Listen Mode, it reappears as a new enabled target.

The app may continue to show current live LV1 scene as status, but it must not block scene config editing based on the live scene.

## Commands

Phase 4 adds or updates Tauri commands for UI actions:

- `select_scene_config(scene_id) -> AppViewState`
- `set_scene_fade_enabled(scene_id, enabled) -> AppViewState`
- `set_listen_mode(active) -> AppViewState`
- `set_fade_target_enabled(scene_id, group, channel, enabled) -> AppViewState`
- `remove_fade_target(scene_id, group, channel) -> AppViewState`

Command behavior:

- `select_scene_config` fails while Listen Mode is active.
- `set_listen_mode(true)` fails if no scene is selected.
- `set_listen_mode(true)` fails if no LV1 channels are known.
- `set_listen_mode(false)` only turns off writes. It does not clear targets or selection.
- Target update commands fail if the scene or target does not exist, so UI bugs are visible.

## Errors And Safety

Phase 4 is capture-only and must not send fader commands to LV1.

Safety behavior:

- Disconnecting LV1 turns Listen Mode off and preserves in-memory scene configs.
- Selecting another scene while Listen Mode is active is blocked.
- Starting Listen Mode without a selected scene is blocked.
- Starting Listen Mode without known LV1 channels is blocked.
- Fader events while Listen Mode is inactive do not modify scene configs.
- Fader events for unknown channels are ignored. Unknown channel warnings should be rate-limited or logged at most once per group/channel to avoid log spam.
- Removing or toggling an unknown target returns a command error.

## Testing

Rust unit tests should cover the state transformation behavior without requiring LV1 hardware:

- Scene list reconciliation creates one config per scene.
- Existing scene config keeps `fadeEnabled` and `fadeTargets` after scene list update.
- New scenes get disabled empty configs.
- Removed scenes are removed from Phase 4 in-memory state.
- Selected scene cannot change while Listen Mode is active.
- Listen Mode cannot start without a selected scene.
- Listen Mode cannot start without known channels.
- Fader events while Listen Mode is inactive do not write targets.
- Fader events while Listen Mode is active write a new target to the selected scene.
- Repeated fader events update the same target's `targetDb` and `updatedAt`.
- Removed targets can be recaptured by later fader events while Listen Mode remains active.
- LV1 disconnect turns Listen Mode off and preserves configs.
- Unknown channel fader events are ignored.

Frontend verification should include the TypeScript build and a manual check of the Scene tab in the Tauri shell if feasible.

## Out Of Scope

Phase 4 does not include:

- JSON project save/load.
- Durable scene identity for renamed or reordered scenes.
- Scene mismatch warnings.
- Automatic fade triggering on LV1 scene recall.
- HTTP/WebSocket APIs.
- Sending fader commands from Listen Mode.
- Start-value or delta storage for captured targets.

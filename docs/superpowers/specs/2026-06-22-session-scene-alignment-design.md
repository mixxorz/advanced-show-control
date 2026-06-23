# Session Scene Alignment Design

## Purpose

The app stores fade settings for LV1 scenes. LV1 owns the actual scene list. The app must preserve saved fade settings when the LV1 scene list changes. It should continue to track ordinary LV1 scene-list edits, while preserving deleted or ambiguous saved configs as unlinked rows instead of dropping data.

The model is simple:

- Every app scene config has an internal UUID used only by the app.
- A scene config is linked when it has a current LV1 scene index and name.
- A scene config is unlinked when its LV1 scene index is `null`.
- The app runs the same pure aligner every time the active LV1 scene list changes.
- The app preserves deleted or ambiguous saved configs as unlinked rows.

## Scene Config Model

Delete `scene_id` from the Rust model, TypeScript model, projection, commands, components, tests, and show-file import/export. Do not leave it as unused compatibility state.

Add an internal scene UUID to every scene config:

- Rust field: `internal_scene_id: Uuid`
- Serialized/frontend field: `internalSceneId: string`

Use `internal_scene_id` for app tracking only:

- React row keys.
- Selected scene state.
- Cued scene state.
- Command arguments.
- Link/delete/overwrite operations.
- Show-file persistence.

Do not use `internal_scene_id` for LV1 matching. It is not LV1 identity and it is not alignment evidence.

Change `scene_index` to nullable:

- Rust field: `scene_index: Option<i32>`
- Serialized/frontend field: `sceneIndex: number | null`

Linked config:

- `scene_index = Some(index)`
- `scene_name = current LV1 scene name`
- Has an `internal_scene_id`
- Can be selected and edited
- Can be stored, cued, recalled, and used by recall automation after existing safety checks pass

Unlinked config:

- `scene_index = None`
- `scene_name = saved scene name for display`
- Has an `internal_scene_id`
- Preserves duration, scope toggles, scoped channels, and stored channel targets
- Can be selected and edited
- Can be linked to an LV1 scene
- Can be deleted
- Can be saved and reopened
- Cannot be stored, cued, recalled, or used by recall automation

Show files must persist `internalSceneId` for every scene config. Legacy show files without `internalSceneId` get generated UUIDs during import and are marked dirty so the IDs are saved next time.

## Aligner

Create one pure function, likely in `show/scene_alignment.rs`:

```rust
fn align_scene_configs(
    configs: Vec<SceneConfig>,
    lv1_scenes: &[SceneListEntry],
) -> Vec<SceneConfig>
```

No report object is required for this feature. The output vector is enough. Dirty state is derived by comparing the returned vector to the previous vector.

The show actor shall call this function directly when it receives an active-generation `Lv1Event::SceneListChanged`. Do not keep `ShowState::reconcile_scene_fade_configs` as an indirection layer. Delete it and move its algorithm tests to `scene_alignment.rs`.

Keep the existing scene-list change classifier shape: `Noop`, `Rename`, `Move`, `Insert`, `Delete`, and `Ambiguous`. Move the classifier into the alignment module with the aligner.

Keep a diagnostic helper in the alignment module. The show actor shall log one human-readable `INFO` tracing event when alignment changes scene configs. This diagnostic is for logs only. It is not returned to callers, not projected as frontend state, and not used by the UI.

Session import may also call `align_scene_configs` if a current LV1 scene list is already available at load time. This is an optimization only. The required behavior is satisfied by running the aligner on every active LV1 scene-list change, because LV1 publishes a scene list on connection and after list edits.

There is no separate “not yet checked” state in this feature. If LV1 is disconnected, there is no LV1 scene list to align against. Once an active LV1 scene list arrives, the show actor runs the aligner.

## Alignment Rules

The aligner must apply these rules exactly.

Definitions:

- A linked config has `scene_index = Some(index)`.
- An unlinked config has `scene_index = None`.
- A scene name is unique in a list when it appears exactly once in that list.
- A blank file scene config has no configured fade behavior: default duration, no stored channel targets, no scoped channels, and default scope toggles.

First classify the transition from previous linked configs to the incoming LV1 scene list. Existing unlinked configs are excluded from classification and carried through unchanged.

Classification rules match the current implementation:

1. `Noop`: previous linked entries and LV1 entries are identical.
2. `Rename`: list lengths are equal, exactly one position changed, and the changed entry kept the same LV1 index.
3. `Move`: list lengths are equal and exactly one name-based move candidate transforms the previous linked list into the incoming LV1 list.
4. `Insert`: incoming LV1 list has one additional entry and exactly one name-based insert candidate transforms the previous linked list into the incoming LV1 list.
5. `Delete`: incoming LV1 list has one fewer entry and exactly one name-based delete candidate transforms the previous linked list into the incoming LV1 list.
6. `Ambiguous`: every other transition.

Application rules:

1. Existing unlinked configs remain unlinked.
2. `Noop` preserves linked configs unchanged.
3. `Rename` applies positional mapping. The renamed config keeps its internal UUID and fade data; its `scene_name` is updated from the incoming LV1 scene. Other linked configs keep their UUIDs and fade data.
4. `Move` uses the existing name-keyed FIFO matching strategy. Matched configs keep their internal UUIDs and fade data, and their LV1 locators are updated from the incoming LV1 scenes.
5. `Insert` uses the existing name-keyed FIFO matching strategy for old configs and creates one default linked config with a new internal UUID for the inserted LV1 scene.
6. `Delete` uses name-keyed FIFO matching for LV1 scenes that still exist. The deleted saved config is preserved as unlinked with `scene_index = None`.
7. `Ambiguous` avoids FIFO guessing. First preserve exact current `(index, name)` matches. For remaining entries, if a scene name appears exactly once among previous linked configs and exactly once in the current LV1 scene list, treat it as the same LV1 scene even if the index changed. Previous linked configs that cannot be matched by exact identity or unique name become unlinked with `scene_index = None`. Current LV1 scenes that do not receive an existing config get default linked configs with new internal UUIDs.
8. New default configs use default duration, empty stored targets, default scope toggles, and current LV1 locator fields.
9. Sort the returned configs with linked configs in current LV1 scene-list order, followed by unlinked configs in their prior relative order.
10. Never delete a saved config during alignment.

Duplicate scene names can make name-keyed FIFO matching semantically uncertain. The app should warn users when duplicate scene names exist, but the aligner remains deterministic and keeps the FIFO behavior for `Move`, `Insert`, and `Delete` classifications.

When importing a session file, blank file scene configs are dropped before alignment. These rows do not carry app-managed fade behavior and must not participate as saved alignment evidence. If LV1 currently contains the same scene, the aligner may still create a fresh default linked config from the LV1 scene list.

## Link And Delete UX

The scene list shows unlinked configs as normal selectable rows with additional warning styling:

- Alert marker.
- Red text.
- Saved scene name.
- Scene number display as `--`.

Selecting an unlinked row shows the normal scene edit controls plus an unlinked section.

Normal controls remain visible. Store, Cue, and Recall buttons are disabled for unlinked configs. Duration and scope editing remain enabled.

The unlinked section contains:

- Dropdown of current LV1 scenes.
- `Link to LV1 Scene` action.
- `Delete` action.

If the target LV1 scene has no existing config, `Link to LV1 Scene` links the selected unlinked config directly.

If the target LV1 scene already has a config, show a confirmation modal. Confirming overwrites the existing target config with the selected unlinked config. There is no merge behavior.

Delete removes the selected config from the session.

## Commands

All scene config commands target `internal_scene_id`.

Add show-owned commands:

```rust
LinkSceneConfig {
    source_internal_scene_id: Uuid,
    target_scene_index: i32,
    overwrite_existing: bool,
    reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
}

DeleteSceneConfig {
    internal_scene_id: Uuid,
    reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
}
```

`LinkSceneConfig` rules:

1. Source config must exist.
2. Source config must be unlinked.
3. Current LV1 scene list must be available.
4. Target LV1 scene index must exist in the current LV1 scene list.
5. If another config already targets the LV1 scene and `overwrite_existing = false`, return `Link blocked: target scene already has a config`.
6. If another config already targets the LV1 scene and `overwrite_existing = true`, delete the existing target config.
7. Set the source config's `scene_index` and `scene_name` from the target LV1 scene.
8. Preserve the source config's internal UUID, duration, scope toggles, scoped channels, and stored channel targets.
9. Mark the session dirty and publish show state when changed.

`DeleteSceneConfig` rules:

1. Config must exist.
2. Delete the config by internal UUID.
3. Clear selected scene if it points to the deleted UUID.
4. Clear cued scene if it points to the deleted UUID.
5. Mark the session dirty and publish show state when changed.

Update existing commands:

- `SelectSceneConfig` uses internal UUID.
- `SetSceneDuration` uses internal UUID.
- Scope toggle and channel scope commands use internal UUID.
- `StoreSceneConfigFromCurrentLv1` uses internal UUID.
- `CueScene` uses internal UUID.
- Explicit `RecallScene` uses internal UUID.

Unlinked command behavior:

- Store rejects unlinked configs with `Store blocked: scene is unlinked`.
- Cue rejects unlinked configs with `Cue blocked: scene is unlinked`.
- Recall rejects unlinked configs with `Recall blocked: scene is unlinked`.
- Duration and scope-editing commands shall continue to work for unlinked configs.

## Safety

Alignment never sends LV1 commands and never starts fades.

Recall automation only considers linked configs. It must ignore unlinked configs.

Existing safety checks remain unchanged: lockout, connection state, fresh LV1 state, exact current scene identity, channel topology, stored target validation, manual override behavior, disconnect behavior, and generation guards.

The UI disables Store, Cue, and Recall for unlinked configs. Backend commands still reject unlinked Store, Cue, and Recall requests.

Duplicate scene names should be visible as a warning in the scene list because they reduce the user's ability to reason about name-keyed tracking.

## Tests

Delete obsolete `ShowState::reconcile_scene_fade_configs` tests after equivalent pure aligner tests exist.

Aligner tests:

- Exact index/name match preserves internal UUID and fade data.
- Single same-index rename preserves internal UUID and fade data while updating scene name.
- Single move preserves internal UUID and fade data while updating LV1 locator fields.
- Single insert preserves existing configs and creates one default linked config.
- Single delete preserves the deleted saved config as unlinked.
- Ambiguous multi-rename preserves unmatched old configs as unlinked and creates defaults for unmatched LV1 scenes.
- Ambiguous multi-operation change does not use FIFO guessing.
- Duplicate names keep deterministic FIFO behavior for classified move/insert/delete cases.
- Unlinked config remains unlinked across repeated alignment.
- New LV1 scene creates default linked config with new UUID.
- Returned configs are sorted linked-in-LV1-order then unlinked-in-prior-order.
- Alignment never deletes saved configs.
- Alignment diagnostic is logged as an INFO trace when configs change.

Load/connect tests:

- Legacy import generates missing internal UUIDs and marks dirty.
- Connected load can call the aligner using current LV1 scenes.
- Active-generation scene-list event aligns loaded configs.
- Stale-generation scene-list event does not align loaded configs.
- Alignment changes mark the session dirty.

Command tests:

- Store rejects unlinked config.
- Cue rejects unlinked config.
- Recall rejects unlinked config.
- Duration edit works for unlinked config.
- Scope edit works for unlinked config.
- Link to LV1 scene without existing config links the source and preserves fade data.
- Link to LV1 scene with existing config requires overwrite confirmation.
- Link with overwrite deletes the existing target config and links the source.
- Delete removes config and clears selected/cued UUID if needed.

Persistence tests:

- Internal UUID serializes to show files.
- Internal UUID round-trips through save/load.
- Unlinked configs round-trip through save/load with `sceneIndex: null`.
- `sceneId` is not serialized.

Frontend tests:

- Scene list row renders unlinked config with alert marker, red text, and `--` scene number.
- Selected unlinked config shows normal edit controls plus unlinked link/delete section.
- Store, Cue, and Recall buttons are disabled for unlinked configs.
- Duration and scope controls remain enabled for unlinked configs.
- Link action without existing target config does not show overwrite confirmation.
- Link action with existing target config shows overwrite confirmation.
- Duplicate scene names show a warning.

## Non-Goals

- No alignment report object in this feature.
- No fuzzy matching.
- No field-level merge.
- No new disconnected-session automation state.
- No LV1 protocol changes.
- No weakening of recall automation safety rules.

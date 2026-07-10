# Cue Lists Design

## Purpose

Cue lists let engineers build show-order lists from the existing LV1 scene library without changing scene order on the console. A cue list entry references an app scene config and can appear multiple times in the same cue list. Recalling a cue uses the existing scene recall path so LV1 recall validation, lockout, stale-state checks, generation guards, fade safety, and app-managed fade behavior remain centralized.

Cue lists also replace the older scene cue workflow. The app should have one cue concept: a cued cue-list entry. The global Go action recalls that entry and advances to the next cue immediately after a successful recall.

## Scope

In scope:

- Multiple named cue lists per `.ascs` session.
- Create, rename, delete, and drag-reorder cue lists.
- Drag scenes from the scene library into the active cue list.
- Allow the same scene to appear multiple times as separate cue entries.
- Drag-reorder cue entries inside the active cue list.
- Cue an entry without recalling LV1.
- Recall the cued entry through the global Go action.
- Always auto-next immediately after successful cue recall.
- Persist cue lists inside `.ascs` session files.
- Remove the existing scene cue state and remove the auto-cue-next setting.

Out of scope:

- Cue entry custom names, notes, colors, or per-entry metadata.
- Add buttons for scene-to-cue-list insertion. Scene insertion is drag-and-drop only.
- Delete confirmation dialogs for cue entries. Cue-list deletion does require confirmation.
- Separate cue-list files outside `.ascs`.
- Recalling a cue from a non-active cue list.
- Event automation, external control, Stream Deck integration, or documentation-site work.

## Architecture

Add a dedicated `cue_lists` backend domain under `src-tauri/src/cue_lists/`. It follows the existing actor conventions:

- `commands.rs` defines explicit mailbox commands.
- `events.rs` defines `CueListsEvent` facts.
- `handle.rs` remains a dumb cloneable sender.
- `state.rs` owns pure cue-list document mutation.
- `types.rs` owns cue-list document types.
- `actor.rs` owns command handling and recall coordination.

The cue-list actor is app-lifetime session state. It owns cue-list documents, active cue-list state, and cued cue-entry state. It publishes state facts through `AppEventBus`; it does not emit `app-status-changed` directly.

Cue recall is routed through the existing `scenes` actor with `ScenesCommand::RecallScene`. The cue-list actor never talks to LV1 or Fade directly and does not duplicate scene recall safety rules. A blocked scene recall remains blocked by the existing scene recall path. The cue-list actor advances to the next cue only after `ScenesCommand::RecallScene` returns success.

The projector consumes `CueListsEvent::StateChanged` and adds cue-list state to `AppViewState`. The frontend continues to receive backend state only through `app-status-changed`.

## Data Model

The cue-list document contains:

```text
cueLists: CueList[]
activeCueListId: Uuid | null
cuedCueEntryId: Uuid | null
```

Each cue list contains:

```text
id: Uuid
name: string
entries: CueEntry[]
```

Each cue entry contains:

```text
id: Uuid
sceneInternalId: Uuid
```

Rules:

- Cue lists always auto-next after successful recall. There is no cue-list auto-next setting.
- Changing `activeCueListId` clears `cuedCueEntryId`.
- Deleting the cued entry clears `cuedCueEntryId`.
- Deleting the active cue list clears `activeCueListId` and `cuedCueEntryId`.
- Reordering cue lists preserves the active cue list by ID.
- Reordering cue entries preserves the cued entry by ID.
- Repeated uses of the same scene are separate cue entries with distinct IDs.
- Cue entry labels are resolved from `sceneConfigs` by `sceneInternalId`.

## Commands And Behavior

Backend cue-list commands:

```text
CreateCueList { name }
RenameCueList { cue_list_id, name }
DeleteCueList { cue_list_id }
ReorderCueLists { ordered_ids }
SetActiveCueList { cue_list_id | null }
AddSceneToActiveCueList { scene_internal_id, insert_index }
RemoveCueEntry { cue_entry_id }
ReorderCueEntries { ordered_entry_ids }
CueEntry { cue_entry_id | null }
RecallCuedCue
InitialProjectionState { reply }
ReplaceCueListDocument { document, persisted_cue_list_edit }
GetCueListDocument { reply }
Shutdown
```

Behavior:

- Creating any cue list makes it active and leaves `cuedCueEntryId` null.
- Renaming trims names and rejects blank names.
- Deleting cue entries happens immediately without confirmation.
- Deleting cue lists requires an app-owned confirmation modal before sending `DeleteCueList`.
- `SetActiveCueList` clears `cuedCueEntryId`, including when the active list is set to null.
- `AddSceneToActiveCueList` requires an active cue list and inserts a new entry at the requested position.
- `CueEntry` sets the entry that the global Go action recalls. It does not recall LV1.
- `CueEntry` rejects entries that are not in the active cue list.
- `RecallCuedCue` blocks if there is no active cue list, no cued cue entry, the cued entry is not in the active cue list, the scene reference is missing, or the existing scene recall path rejects the scene recall.
- Successful `RecallCuedCue` advances `cuedCueEntryId` to the next entry ID immediately, or clears it when the recalled entry was last.
- Blocked recall does not advance.
- Cue-list edits publish state changes with a persisted-edit flag so existing session dirty-state handling can mark the `.ascs` session dirty.

## Existing Cue Workflow Removal

The existing scene-level cue state is removed to avoid two competing cue concepts.

Remove:

- `cued_scene_internal_id` from `ScenesState`, `SceneDocument`, scene projection, and show-file scene persistence.
- `ScenesCommand::CueScene` and related command adapters/frontend command wrappers.
- Scene cue UI affordances from the Scenes tab.
- Any global Go routing to a scene-level cued scene.

Keep direct scene recall from the Scenes tab as an explicit scene action/navigation path. Direct scene recall still routes through `ScenesCommand::RecallScene` and remains subject to existing safety checks.

## Settings Removal

Remove `autoCueNextSceneOnGo` from:

- `AppSettings` types and defaults.
- Settings persistence and validation.
- Projected `AppViewState.settings`.
- Settings UI and Storybook/test fixtures.
- Frontend tests and command typing.

There is no replacement cue-list setting because cue lists always auto-next.

## Persistence

The `.ascs` schema moves to version 2 and includes cue-list document state. Existing v1 files import with an empty cue-list document. The old scene cue value from v1 is ignored because scene cue is no longer a supported workflow.

Import/export responsibilities:

- Show-file export serializes scene configs plus cue-list document state.
- Show-file import validates schema version and builds scene and cue-list documents.
- Loading a session sends `ReplaceCueListDocument` with `persisted_cue_list_edit` false so loading a clean file does not mark the session dirty solely because cue lists were projected.
- Cue entries whose `sceneInternalId` does not exist in imported scene configs remain in the cue list but are invalid/unrecallable until removed or repaired by later reconciliation work.
- Importing invalid cue-list references should make the invalid state visible in projection/UI rather than silently deleting entries.

## UI Design

The Cue Lists tab replaces the placeholder with a two-column layout:

```text
left 1/3: Scene Library
main 2/3: Active Cue List
```

The scene library shows app scene configs in console order and supports dragging scenes into the active cue list. There is no Add button.

The main pane header contains:

- Cue-list dropdown for `activeCueListId`.
- `New Cue List` action.
- `Manage Cue Lists` button.
- Cued/next/status text.

The main pane does not add a local Go button. The existing app Go action recalls the cued cue-list entry.

The cue list body shows numbered cue entries, highlights the cued cue, marks invalid/missing scene references, and supports drag-and-drop reordering. Clicking an entry cues it without recalling LV1.

`New Cue List` is available both from the main cue-list controls and from inside `Manage Cue Lists`. Both entry points open the same app-owned modal that asks for the cue-list name. The app must not use `window.alert`, `window.confirm`, or `window.prompt`; prompts and confirmations are React/Tauri UI, not browser-native dialogs.

`Manage Cue Lists` opens a modal for creating, renaming, deleting, and drag-reordering cue lists. Cue-list deletion opens an app-owned confirmation modal. Cue-entry deletion remains immediate with no confirmation.

Modal components should be reused where practical. Do not duplicate modal implementations unless the interaction is different enough that reuse would make the component harder to understand.

On narrow layouts, the scene library stacks above the active cue list while preserving the same responsibilities.

## Projection And Logging

`AppViewState` gains cue-list projection fields, including cue lists, `activeCueListId`, `cuedCueEntryId`, and last cue recall status/block reason.

User-facing logs:

- `INFO` for creating, renaming, deleting, and reordering cue lists when useful to the operator.
- `INFO` for successful cue recall, including cue position and scene label.
- `WARN` for blocked cue recall, missing active cue list, missing cued entry, invalid cue entry, or missing scene reference.

Avoid duplicate logs across cue-list and scene recall layers. Cue-list logs should describe cue-list context; existing scene recall logs should continue to describe scene safety decisions.

## Testing

Rust test styles:

- Pure unit tests for cue-list state changes, active-list changes clearing `cuedCueEntryId`, delete behavior, list reorder, entry reorder, invalid references, and auto-next target selection.
- Actor tests for command handling through the cue-list actor mailbox, event publication through `AppEventBus`, session dirty signaling, and `RecallCuedCue` routing through `ScenesCommand::RecallScene`.
- Existing scene recall actor tests remain the safety authority. Cue-list actor tests verify cue recall cannot bypass the existing scene recall command result.

Frontend tests:

- Vitest and Storybook coverage for Cue Lists tab rendering, active-list dropdown, cue entry status, invalid entries, drag/drop command calls, and management modal behavior.
- Tests for removing the Settings auto-cue-next control and associated settings field.

Persistence tests:

- Show-file export includes schema version 2 cue-list data.
- Show-file import loads cue lists, active cue list, and cued cue entry.
- v1 show files import with empty cue lists and ignored legacy scene cue state.

## Open Follow-Up Work

Later releases can add richer cue-entry metadata, external cue-list control, Stream Deck feedback, documentation-site content, and repair/remap workflows for cue entries whose referenced scenes are missing. These are not part of this implementation slice.

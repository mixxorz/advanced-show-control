# Cue List Review Remediation Design

## Purpose

Harden cue-list operation for live use by restoring the cleanup, generation, and interaction guarantees lost when cue ownership moved from the scenes actor into the cue-lists actor.

The remediation preserves the current persistence model: cueing and successful GO advancement update saved session state and therefore mark the show dirty.

## Scope

This work covers:

- reconciliation between scene documents and cue-list references;
- safe connected-runtime peer installation;
- GO validity and duplicate-submission protection;
- active-list and local-selection behavior;
- cue status presentation in the footer and scene editor;
- native button event forwarding;
- session-load normalization and schema-v1 migration visibility.

This work does not prune missing-scene cue entries, change cue-position persistence, refactor unrelated cue-list command adapters, or optimize cue-row rendering.

## Ownership And Architecture

The `cue_lists` actor remains the sole owner of cue-list documents, the active cue-list ID, and the cued cue-entry ID. It observes scene projection facts from `AppEventBus` so it can reconcile scene references without moving cue ownership back into `scenes` or placing cue-list policy in `show`.

The `scenes` actor remains the owner of scene documents and recall safety. Cue recall continues to route through `ScenesCommand::RecallScene`, preserving fresh LV1 state checks, lockout checks, exact scene identity validation, and generation guards.

The frontend derives whether GO is armed from projected backend state. It does not implement recall safety policy, but it prevents submissions that are already known to be invalid and prevents duplicate submissions while a recall command is unresolved.

## Cue Reference Reconciliation

Missing scene references are preserved in cue-list entries. This retains show-order intent for future scene reconciliation and allows the UI to display `Missing scene` rather than destructively deleting operator data.

The cue-lists actor maintains the latest set of scene internal IDs from active-generation `ScenesEvent::StateChanged` facts. When that set changes:

1. Entries whose scene IDs are absent remain unchanged.
2. If the currently cued entry references an absent scene, `cued_cue_entry_id` is cleared.
3. Clearing the cue publishes a cue-list state change as a persisted cue-list edit, so the show becomes dirty.
4. A `WARN` tracing event reports that the cued entry was cleared because its scene is unavailable. The message must be complete enough for the frontend log UI.
5. Scene changes that do not invalidate the current cue publish no cue-list event and produce no warning.
6. Stale-generation scene events are ignored.

The same invariant applies when replacing a cue-list document during session load. A cued entry is valid only when:

- `active_cue_list_id` identifies an existing cue list;
- `cued_cue_entry_id` identifies an entry in that active list; and
- that entry references a scene in the imported, post-alignment scene document.

If the active cue-list ID is invalid, both active and cued IDs are cleared. If only the cued entry is invalid, only the cued ID is cleared. Missing entries remain in their lists.

## Session Migration

Schema-v2 session files continue to store cue lists, the active cue-list ID, and the cued cue-entry ID.

Schema-v1 files may contain the former `cuedSceneInternalId`, but they contain no cue list or cue-entry identity to which that scene can be mapped safely. Loading schema v1 therefore does not synthesize a cue list. If the old field is present, the load validation report records that the pre-armed scene cue could not be migrated, and a visible warning explains that the cue was cleared.

Session import reconciliation occurs against the final post-import scene document, after blank-scene pruning, internal-ID generation, and scene alignment. This avoids validating cue references against scene IDs that will not be installed.

## Runtime Peer Safety

Building a connected runtime must not mutate app-lifetime cue-list peers before the lifecycle transaction confirms that the runtime generation is current.

The connected runtime carries its candidate `ScenesHandle` until `install_runtime_transaction` succeeds. Only the accepted generation installs that handle into `CueListsPeers`. A rejected stale generation aborts its own handles without changing the current cue-list peer. Disconnect and accepted-runtime failure paths continue to clear the installed peer.

This guarantees that a stale connection attempt cannot replace a current cue recall route with a dead scenes actor.

## GO Behavior

GO is enabled only when all projected preconditions hold:

- an active cue list exists;
- `cued_cue_entry_id` resolves to an entry in that list;
- the entry's scene internal ID resolves to a projected scene config;
- no GO command is currently in flight.

Pressing GO sets a frontend-local in-flight flag before dispatching `recallCuedCue`. Pointer clicks and keyboard repeat cannot dispatch another GO until the returned promise settles. The flag clears on both success and failure.

The backend actor remains sequential and advances the persisted cued pointer only after `ScenesCommand::RecallScene` succeeds. Successful GO continues to publish `persisted_cue_list_edit: true` and mark the show dirty.

The footer's Cued cell displays only the scene resolved through the active cue list and cued entry. It must not fall back to `selected_scene_internal_id`. With no valid cue, it displays `---` in the default tone and GO is disabled.

Command failures remain routed through the existing app command-error mechanism; preventing known-invalid GO attempts reduces invisible failures but does not replace backend validation.

## Selection And Active-List Behavior

Calling `set_active_cue_list` with the already-active cue-list ID is a no-op. It does not clear the cued entry, publish a state change, or mark the show dirty. Selecting a different list still clears the cued entry.

`CueListsTab` keeps row selection as local presentation state. On each projection update, it clears `selectedCueEntryId` when the selected entry is no longer in the active cue list. The Cue button is enabled only for a selection that resolves in the current active list.

Deleting the selected entry or changing the active cue list therefore disables Cue without sending a stale ID to the backend.

## Component Corrections

`ConsoleIconButton` forwards all standard button attributes after removing its custom `size` and `variant` props. This preserves `onPointerDown`, keyboard, data, and accessibility attributes supplied by callers. The component continues to default `type` to `button`.

`SceneEditor` derives `SelectedSceneHeader.cued` from the active cue list's valid cued entry. A selected scene is styled as cued only when its internal ID matches that entry's scene internal ID. Selection alone never implies cued state.

## Logging

The cue-lists actor emits one warning when reconciliation clears an invalid current cue:

- stable event name: `cue_cleared_missing_scene`;
- structured fields: cue-list ID, cue-entry ID, and missing scene internal ID when available;
- user-facing message: `Cued entry cleared because its scene is unavailable.`

Schema-v1 cue migration loss emits a warning at the session-load ownership seam rather than duplicating the cue-lists reconciliation warning. The load report and log message identify that an old pre-armed scene cue could not be migrated because schema v1 had no cue-list entry.

No log is emitted for preserved non-cued missing entries or same-list activation no-ops.

## Testing Strategy

All behavior changes follow test-driven development.

### Rust Pure Unit Tests

- Replacing a document clears an invalid active cue-list ID and its cued ID.
- Replacing a document clears a cued entry that references a missing scene while preserving the entry.
- Replacing a document preserves a valid active and cued entry.
- Setting the already-active cue list is a no-op and preserves the cue.
- Changing active cue lists clears the cue.
- Schema-v1 import reports an unmigratable legacy scene cue.

### Rust Actor Tests

- Through the actor mailbox and `AppEventBus`, an active-generation scene state change clears an invalid current cue, publishes a persisted edit, and emits the warning.
- A scene state change preserves the cue and emits no cue-list state event when the referenced scene remains valid.
- A stale-generation scene event cannot clear the cue.
- A stale connected-runtime installation cannot replace the cue-lists actor's current scenes peer.
- Cue recall remains routed through `ScenesCommand::RecallScene` and advances only on success.

### Frontend Unit Tests

- GO is disabled with no cued entry, a missing active list, a missing entry, or a missing scene config.
- The Cued cell does not display the selected scene as a fallback.
- A pending GO disables the button and suppresses a second submission until settlement.
- GO re-enables after command success and failure when a valid cue remains projected.
- Reselecting the active cue list does not request an activation command.
- Deleting or switching away from a selected cue entry disables Cue.
- `ConsoleIconButton` forwards `onPointerDown`.
- `SceneEditor` marks only the actual cued scene as cued.

### Broader Verification

Run focused Rust and frontend tests during each red-green cycle. Before completion, run `make check`. Run visual regression tests only if the corrected status states intentionally change snapshots. Hardware smoke remains optional because these changes can be proven through actor and frontend seams without an LV1-compatible target.

## Acceptance Criteria

1. Rapid repeated GO input can dispatch at most one recall while the first command is unresolved.
2. GO cannot be invoked from the UI without a valid projected cue and scene reference.
3. Scene deletion, overwrite, realignment, or session load clears an invalid current cue without deleting missing-scene entries.
4. Invalid cue cleanup is persisted, marks the show dirty, and is visible through one warning.
5. Clicking the active cue list does not lose the operator's cue position.
6. Stale row selection cannot submit an obsolete cue-entry ID.
7. The footer and scene editor display cue status only from cue-list state.
8. Native button event handlers used to isolate drag and delete interactions are preserved.
9. A stale runtime generation cannot replace the current cue recall peer.
10. Schema-v1 pre-armed cue loss is reported rather than silently discarded.
11. Successful GO advancement continues to mark the show dirty.

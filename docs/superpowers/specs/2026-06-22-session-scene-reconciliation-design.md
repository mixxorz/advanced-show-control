# Session Scene Reconciliation Design

## Purpose

Session load and connect-time LV1 sync should use the same scene reconciliation behavior as live LV1 scene-list tracking. A session opened before or after LV1 connection should preserve app-managed fade configuration when LV1 scenes were renamed, moved, inserted, or deleted and the existing reconciliation strategy can preserve that configuration deterministically.

LV1 remains the source of truth for scene creation, order, names, indexes, and recall. The app only remaps its scene fade overlay onto the current LV1 scene list.

## Current Problem

Live scene-list updates already reconcile `SceneConfig` values against LV1 `SceneListEntry` values. Load-time validation does not use that path. It keeps only exact `(scene_index, scene_name)` matches and drops everything else.

That means a saved session can lose fade configuration on load even though the live reconciliation strategy would have preserved the same configuration after a rename or move.

There is also a connect-time requirement: if scene configs are already loaded before connecting to LV1, the first current LV1 scene list for the active generation must reconcile those configs. This should use the same shared reconciler rather than a separate load-only path.

## Design

Extract scene reconciliation into one pure reusable unit, likely `show/scene_reconciliation.rs`.

The reconciler accepts:

- Existing app-managed `Vec<SceneConfig>` values.
- Current LV1 `Vec<SceneListEntry>` values.

The reconciler returns:

- Reconciled `Vec<SceneConfig>` values.
- A reconciliation report describing the applied change classification and user-visible consequences.

`ShowState::reconcile_scene_fade_configs` becomes a thin state wrapper around this shared reconciler. Session import first converts file scene records into `SceneConfig` values, then calls the same reconciler against the current LV1 scene list before building the loaded `ShowDocument`. Connect-time LV1 scene-list handling continues to call the same `ShowState` wrapper, so sessions loaded before connection reconcile as soon as the active LV1 scene list arrives.

## Reconciliation Behavior

The shared reconciler preserves the current live behavior:

- No-op scene lists preserve all config data unchanged.
- A single same-index rename preserves config data positionally and updates locator fields.
- Moves, inserts, deletes, and ambiguous changes use name-keyed FIFO matching.
- Existing configs with matching names preserve duration, scoped channels, scope toggles, and stored channel targets.
- New LV1 scene names receive default scene configs.
- Saved configs that do not map to the current LV1 scene list are dropped.
- Duplicate scene names remain deterministic through FIFO matching but are treated as ambiguous for reporting.

The reconciler always updates `scene_id`, `scene_index`, and `scene_name` from the current LV1 scene list for kept configs.

## Load-Time Behavior

Opening a session requires a non-empty current LV1 scene list, as it does today.

Load-time import should validate schema version, convert saved scene records into app `SceneConfig` values, reconcile those configs against the current LV1 scene list, then load the reconciled document into `ShowState`.

The loaded session should be marked dirty when reconciliation changes the loaded data relative to the file. This includes locator updates, added default configs for new LV1 scenes, removed unmapped configs, and filtered cued scenes.

The selected scene should continue to default to the first loaded scene config after reconciliation.

The cued scene should be preserved only if it still exists after reconciliation. If the cued scene is removed or remapped away, it should be cleared.

## Connect-Time Behavior

When a session already has scene configs and LV1 later connects, the first `SceneListChanged` event for the active generation should reconcile the existing configs against the connected LV1 scene list.

This should happen through the same show-actor scene-list event path used for ordinary live scene-list edits. Generation filtering remains required: stale scene-list events from a previous connection must not reconcile the current show state.

If connect-time reconciliation changes the loaded session data, the session should be marked dirty for the same reasons as load-time reconciliation.

## Reporting And Visibility

Replace the narrow load report concept of only `removed_scenes` with a reconciliation-oriented report.

The report should include enough information for logs and future UI display:

- Change classification, such as no-op, rename, move, insert, delete, or ambiguous fallback.
- Added default scenes.
- Removed or unmapped saved scenes.
- Remapped scenes whose locator changed while preserving config data.
- Duplicate-name ambiguity when present.

Initial implementation can log these facts through existing frontend-facing operational logs. A later UI can display the same report without changing the reconciliation algorithm.

## Safety

Load-time reconciliation must not start fades or send LV1 commands. It only updates app-owned show configuration.

Recall automation continues to validate exact current scene identity, lockout state, LV1 connection state, live channel topology, scoped targets, stored values, and generation before sending fader commands.

Ambiguous FIFO remapping can preserve the wrong duplicate-name scene if two LV1 scenes share a name and have swapped meaning. This is acceptable only because recall safety still requires exact current LV1 scene index and name, and duplicate-name warnings remain visible to the user.

## Testing

Add focused tests for the extracted reconciler covering the existing live cases:

- No-op preservation.
- Same-index rename.
- Move earlier and later.
- Insert default config.
- Delete unmapped config.
- Ambiguous multi-operation fallback.
- Duplicate-name FIFO behavior.
- Cued scene filtering after reconciliation.

Add load-time import tests proving session import uses the shared reconciler rather than exact-match pruning:

- A renamed scene preserves duration and stored channel targets.
- A moved scene preserves duration and stored channel targets.
- A deleted scene is reported as removed or unmapped.
- A new LV1 scene creates a default config and marks the loaded session dirty.
- Duplicate-name ambiguity is reported.

Add connect-time tests proving already-loaded configs reconcile when the active LV1 scene list arrives:

- A session loaded before LV1 connection preserves a renamed scene's config after the first active scene-list event.
- A stale-generation scene-list event does not reconcile loaded configs.
- Connect-time reconciliation marks the session dirty when it changes loaded configs.

## Non-Goals

- No interactive remapping UI in this change.
- No durable app-owned scene UUIDs.
- No fuzzy matching beyond the existing name-keyed FIFO strategy.
- No changes to LV1 protocol handling or recall automation safety rules.

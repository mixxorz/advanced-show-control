# Scene List Tracking Design

## Purpose

The app should preserve stored fade configuration when an LV1 user renames, moves, inserts, or deletes one scene. LV1 remains the source of truth for scene creation, ordering, naming, and recall. The app only updates its stored scene locators so fade configs continue to point at the intended LV1 scenes when the scene-list transition is deterministic.

This design does not add a new durable scene ID. The current `scene_id = "{index}::{name}"` shape remains in place for now.

## Constraints

- LV1 scene-list notifications expose ordered `(index, name)` entries only.
- Users can only perform one scene-list edit at a time.
- A single edit cannot both move and rename a scene.
- Recall automation must continue to validate exact current scene index and name before sending fader commands.
- `Lv1Actor` should emit LV1 facts only. It should not own show-file reconciliation policy.

## Architecture

`Lv1Actor` continues to emit `SceneListChanged(Vec<SceneListEntry>)` whenever LV1 reports the scene list. It does not classify the change.

`ShowState` owns reconciliation. `ShowState::reconcile_scene_fade_configs(new_scenes)` becomes a stateful transform instead of copying exact matches and defaulting everything else. It derives the previous scene list from the current `scene_configs`, compares it with `new_scenes`, classifies the single operation, and updates stored configs accordingly.

`SceneRecallFader` remains unchanged in principle. It validates exact scene identity against fresh LV1 state before starting any fade.

## Change Classification

The reconciliation logic classifies the transition from old ordered entries to new ordered entries as one of:

- `Noop`: old and new ordered `(index, name)` entries match exactly.
- `Rename`: same length, same indexes, exactly one entry changed name.
- `Move`: same length, new order can be produced by removing exactly one old entry and inserting it at another position.
- `Insert`: new list can be produced by inserting exactly one new entry into the old list.
- `Delete`: new list can be produced by deleting exactly one old entry.
- `Ambiguous`: the transition is not explained by exactly one allowed operation.

The classifier should operate on ordered entries, not a name-to-index map, so duplicate names can still be handled when the list transform is uniquely explainable. If duplicate names make the transform indistinguishable, the result is `Ambiguous`.

## Reconciliation Behavior

For `Noop`, keep existing configs as-is.

For `Rename`, preserve the existing scene config at that index and update `scene_name` and `scene_id` to the new locator.

For `Move`, preserve the moved config and all shifted configs. Reorder `scene_configs` to match the new LV1 scene order, then update each config's `scene_index`, `scene_name`, and `scene_id` from the corresponding new scene-list entry.

For `Insert`, create a default `SceneConfig` for the inserted scene. Preserve existing configs, shift them as needed, and update each config's locator fields from the new scene-list entry.

For `Delete`, remove the deleted scene's config. Preserve remaining configs, shift them as needed, and update each config's locator fields from the new scene-list entry.

For `Ambiguous`, fall back to the current exact-match reconciliation behavior. There is no deterministic single-operation transform to apply, so exact `(index, name)` matches keep their existing configs and unmatched entries are treated the same way they are today.

The React scene list should also display a persistent warning when the current scene list is hard to track deterministically, such as when duplicate scene names are present. This warning can be derived entirely in React from the scene list or projected scene config data. It does not need new backend state for the first implementation.

## Safety

Reconciliation only updates stored locators. It does not start fades and does not weaken recall validation.

Recall automation still blocks unless the fresh LV1 snapshot's current scene exactly matches the stored config's `scene_index` and `scene_name`. Ambiguous scene-list transitions must not cause fader commands to be sent.

## Testing

Add focused unit tests for `ShowState::reconcile_scene_fade_configs` covering:

- No-op reconciliation preserves config data.
- Single rename preserves config data and updates locator fields.
- Moving a scene earlier preserves every config and updates indexes.
- Moving a scene later preserves every config and updates indexes.
- Insert creates only one default config and preserves existing configs.
- Delete removes only the deleted config and preserves remaining configs.
- Duplicate names are handled when the single-operation transform is unique.
- Duplicate names produce `Ambiguous` when the moved/deleted/inserted scene cannot be distinguished, then use exact-match reconciliation fallback.
- Multi-operation transitions are `Ambiguous`, then use exact-match reconciliation fallback.
- The React scene list shows a persistent warning when duplicate scene names make tracking harder.

Existing scene recall policy tests should continue to prove exact index/name validation before fade start.

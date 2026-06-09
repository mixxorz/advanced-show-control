# Scene Tracking

The app tracks LV1 scenes for the current active show by listening to LV1 scene-list updates. LV1 remains the source of truth for scene creation, order, naming, and recall. The app only updates its stored scene locators so existing fade configuration follows deterministic LV1 scene edits.

## Current Model

Scene configs currently use the existing locator shape:

```text
scene_id = "{index}::{name}"
```

There is no separate durable app-owned scene ID in this design. `scene_id`, `scene_index`, and `scene_name` are updated together when a scene-list change can be classified deterministically.

## Event Ownership

`Lv1Actor` emits `SceneListChanged(Vec<SceneListEntry>)` as a fact from LV1. It does not infer renames, moves, inserts, or deletes.

`ShowState` owns reconciliation. It compares the previously stored scene config order with the new LV1 scene list and applies the matching single-operation transform.

## Tracked Edits

The app expects one LV1 scene-list edit per scene-list event. A single edit cannot both move and rename a scene.

Supported deterministic edits:

- Rename one scene at the same index.
- Move one scene earlier or later in the list.
- Insert one scene.
- Delete one scene.

When a transform is deterministic, existing fade settings, scoped channels, and stored fader targets are preserved and only the scene locator fields are updated.

## Ambiguous Edits

If the old and new scene lists cannot be explained by one supported operation, reconciliation falls back to exact matching by `(index, name)`, matching the previous behavior. Exact matches keep their config. Unmatched scenes are treated as new or removed according to the current exact-match algorithm.

Duplicate scene names can make some moves or deletes impossible to distinguish from scene-list data alone. The Scene tab should show a persistent warning when the current list has duplicate names or another known hard-to-track condition. The warning is advisory; recall automation still validates exact current scene index and name before sending fader commands.

## Safety

Scene tracking never starts fades. It only updates stored scene locators.

Before automation sends any fader commands, `SceneRecallFader` validates that the fresh LV1 current scene exactly matches the stored config's `scene_index` and `scene_name`. This exact validation remains the safety boundary.

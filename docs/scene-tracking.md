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

## Recall Timing Windows

Scene recall timing includes a short settle delay before policy runs, so the app can wait for the scene change to stabilize before it validates and dispatches fades. While that window is open, arming-baseline suppression can hold off a recall when the recalled scene is still part of the recent baseline state. Repeat suppression can skip a recall when the same scene is recalled again within the configured suppression window. Scene-list edit suppression can also block automation briefly after list churn so the app does not react to intermediate ordering noise. The recall actor then polls `get_lv1_state()` for up to 2 seconds until it sees a fresh snapshot that matches the recalled scene notification; if no matching fresh snapshot arrives in that window, recall is blocked. These are timing gates, not retry guarantees.

## Ambiguous Edits

For changes other than same-index renames, reconciliation matches existing configs to the new LV1 scene list by scene name using FIFO matching. The new LV1 scene list controls final order and indexes. Existing configs with matching names keep their fade settings and receive updated locator fields. New scene names get default configs. Old scene names absent from the new list are dropped.

Duplicate scene names use the same FIFO matching policy: the first new occurrence receives the first old config with that name, the second receives the second, and so on. This is deterministic and avoids silently deleting settings, but it cannot know whether two identically named scenes swapped places. The Scene tab should show a persistent warning when the current list has duplicate names or another known hard-to-track condition. The warning is advisory; recall automation still validates exact current scene index and name before sending fader commands.

## Safety

Scene tracking never starts fades. It only updates stored scene locators.

Before automation sends any fader commands, `SceneRecallFader` validates exact current scene index and name, lockout, connection state, stored scene config, scoped targets, stored fader values, live topology, and generation. Exact scene matching is one safety check, not the whole boundary.

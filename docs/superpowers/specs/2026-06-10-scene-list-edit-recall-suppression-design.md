# Scene List Edit Recall Suppression Design

## Context

The app starts fader fades from LV1 current-scene notifications. `Lv1Actor` parses `/Notify/CurSceneIndex` and `/Notify/Scene/Name` into `Lv1Event::SceneChanged`, and `SceneRecallFader` decides whether that observation should start app-managed recall automation.

Scene-list management can also emit current-scene notifications. A hardware log captured while moving the current scene showed this sequence:

- Startup sync at `6090 ms`: `/Notify/Scene/Name = Song 3`, `/Notify/SceneList`, `/Notify/CurSceneIndex = 4`.
- Current-scene move at `12386-12387 ms`: `/Notify/SceneList` changed `Song 3` from index `4` to index `3`, then `/Notify/Scene/Name = Song 3`, then `/Notify/CurSceneIndex = 3`.

Current code would pair the name and index from the current-scene move and emit `SceneChanged(3, "Song 3")`. If `3::Song 3` is app-managed, recall automation can start even though the user was only managing the scene list.

A separate non-current rename log showed a less dangerous sequence:

- Startup sync emitted `/Notify/Scene/Name = Song 3`, `/Notify/SceneList`, `/Notify/CurSceneIndex = 4`.
- Non-current rename emitted an updated `/Notify/SceneList` and `/Notify/CurSceneIndex = 4`, but did not emit `/Notify/Scene/Name` during the action.

This did not produce an immediate `SceneChanged`, but it still proves scene-list edits can emit partial current-scene data.

## Goals

- Prevent scene-list management actions from starting app-managed fader fades.
- Preserve real scene recall detection.
- Preserve existing startup arming, same-scene repeat, exact scene identity, lockout, generation, disconnect, and fade safety behavior.
- Keep `Lv1Actor` factual. UI and shell state should still receive current scene and scene list facts.
- Keep the first fix small and testable.

## Non-Goals

- Do not suppress `Lv1Event::SceneChanged` or `Lv1Event::SceneListChanged` in `Lv1Actor`.
- Do not classify every scene-list edit as rename, move, insert, or delete for recall gating.
- Do not change show-file scene reconciliation.
- Do not add durable scene IDs.
- Do not weaken exact `index::name` validation.

## Design

Recall gating should treat a changed scene list as a short-lived scene-management window.

`SceneRecallFader` should track the last observed scene list. When it receives `Lv1Event::SceneListChanged(new_list)`:

- If there is no previous scene list, store `new_list` as the baseline and do not suppress recall automation.
- If `new_list` is identical to the previous list, do not suppress recall automation.
- If `new_list` differs from the previous list, store `new_list` and suppress scene recall automation for a short window.

When `SceneRecallFader` receives `Lv1Event::SceneChanged(scene)`, it should apply this gate before recall validation:

- If the current time is inside the scene-list edit suppression window, ignore the scene observation for automation.
- Ignoring means no LV1 snapshot fetch, no show config fetch, no lockout fetch, no skip/block/ready/start log, and no fade start.
- After the window expires, scene observations flow through the existing arming and same-scene repeat logic unchanged.

The suppression window should be long enough to cover the current-scene move burst and small ordering differences, but short enough not to hide later deliberate recalls. A `500 ms` window matches the existing same-scene repeat delay and is conservative relative to the observed current-scene move messages, which arrived within `1 ms`.

The gate should be based on scene-list content changes, not merely the presence of `/Notify/SceneList`. Existing recall evidence shows LV1 can publish `/Notify/SceneList` during a real recall burst. If the scene list is identical, that should not be treated as scene management.

## Data Flow

```text
LV1 TCP messages
-> Lv1Actor parses factual SceneListChanged and SceneChanged events
-> SceneRecallFader tracks scene-list content changes
-> Changed scene list opens a short suppression window
-> SceneChanged inside that window is ignored for automation only
-> SceneChanged outside that window uses existing recall gates and validation
-> FadeEngine receives only validated recall fade commands
```

## Behavioral Cases

Real scene recall without scene-list content changes remains actionable after existing gates.

Real scene recall that republishes an identical scene list remains actionable because identical scene-list messages do not open suppression.

Current-scene move is suppressed because the scene list content changes and the resulting current-scene notification lands inside the suppression window.

Current-scene rename is suppressed for the same reason. Exact scene validation would also protect old `index::name` configs, but suppression avoids treating the management action as a recall at all.

Non-current rename, move, insert, or delete is suppressed if LV1 emits a paired current-scene notification during the edit burst. If it emits only partial current-scene data, no recall happens today, and the suppression window still protects against a delayed paired notification.

Deleting the current scene is suppressed during the edit burst even if LV1 selects another scene as a side effect. This favors safety: moving faders during deletion is riskier than requiring the user to perform a deliberate recall after editing.

## Testing

Add focused tests around `SceneRecallFader` and `SceneRecallState` behavior. The tests should include both small unit-level gate tests and realistic actor event-sequence tests that mirror the captured logs.

Core gate tests:

- First `SceneListChanged` establishes a baseline and does not suppress a later valid recall.
- Identical `SceneListChanged` does not suppress a later valid recall.
- Changed `SceneListChanged` followed by `SceneChanged` inside the suppression window does not start a fade or publish recall logs.
- Changed `SceneListChanged` followed by `SceneChanged` after the suppression window can start a valid fade.

Realistic event-sequence tests:

- Startup sync sequence from the current-scene move log: publish `SceneChanged(4, "Song 3")`, then the initial `SceneListChanged`, then advance through the existing arming delay. This should establish baseline state and not start automation.
- Current-scene move sequence from the hardware log: after arming, publish a changed `SceneListChanged` where `Song 3` moves from index `4` to index `3`, then immediately publish `SceneChanged(3, "Song 3")`. This should not start a fade and should not publish scene recall skip/block/ready/start logs.
- Non-current rename sequence from the hardware log: after arming, publish a changed `SceneListChanged` where `Song 2` becomes `Song 2 -- Changed`, with current scene still `4::Song 3`. If the test includes a paired `SceneChanged(4, "Song 3")` inside the suppression window to model a delayed pair, it should not start a fade.
- Real recall sequence without scene-list content changes: after arming, publish `SceneChanged` for an app-managed scene and confirm the existing valid recall behavior still starts exactly one fade.
- Real recall sequence with an identical scene-list resend: after arming, publish an identical `SceneListChanged`, then `SceneChanged` for an app-managed scene. This should still start exactly one fade because identical scene-list resends do not represent scene management.
- Post-window recovery sequence: after a changed `SceneListChanged`, advance past the suppression window, then publish a valid `SceneChanged`. This should start a fade, proving the gate does not permanently disable recall automation.

Use paused Tokio time where the actor tests already use it. Keep tests focused on whether fade commands and recall events are produced; do not mock raw OSC parsing unless needed.

Because these tests are timing-sensitive, run the targeted test command under a `20` second timeout while developing and before completion. A hung test is a failure. Use existing paused-time scene recall tests as the guide for avoiding real-time sleeps and for advancing Tokio time deterministically.

## Safety Notes

- Suppressed scene observations must not call recall preparation because preparation can log misleading skip/block decisions and fetch fresh LV1 state.
- Generation checks remain required before validation and before fade start.
- Lockout, stale-state, exact scene identity, disconnect, manual override, abort, overlap, and same-scene repeat behavior remain unchanged.
- The gate only suppresses automation. It must not hide current scene or scene list updates from UI state.

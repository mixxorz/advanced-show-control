# Scene Recall Trigger Gating Design

## Context

The app listens to LV1 scene notifications and starts app-managed fader fades when an app-managed scene is recalled. LV1 also sends scene notifications during initial connection state sync and can send several equivalent same-scene notifications during one physical scene recall.

A captured LV1 probe log showed this pattern for one recall of scene `0: My first scene`:

- `12831 ms`: `/Notify/Scene/Name` = `My first scene`
- `12831 ms`: `/Notify/CurSceneIndex` = `0`
- `12924 ms`: `/Notify/Scene/Name` = `My first scene`
- `12926 ms`: `/Notify/CurSceneIndex` = `0`
- `12997 ms`: `/Notify/Scene/Name` = `My first scene`
- `12997 ms`: `/Notify/SceneList`
- `12997 ms`: `/Notify/CurSceneIndex` = `0`
- `12997 ms`: `/Notify/Scene/Name` = `My first scene`

The same log showed initial connection sync reporting the current scene without a user recall:

- `6222 ms`: `/Notify/Scene/Name` = `My first scene`
- `6222 ms`: `/Notify/SceneList`
- `6222 ms`: `/Notify/CurSceneIndex` = `0`

These events are valid LV1 observations, but not every observation should trigger fade automation.

## Goals

- Do not start fades from the first scene identity reported during connect or reconnect.
- Do not finish a fade immediately because LV1 emits duplicate same-scene notifications during one recall burst.
- Preserve intentional same-scene repeat recall as a way to finish that scene's active fade after a small minimum delay.
- Keep LV1 state reporting accurate for UI and other consumers.
- Keep recall trigger policy separate from fade execution semantics.

## Non-Goals

- Do not suppress `Lv1Event::SceneChanged` in `Lv1Actor`.
- Do not add raw OSC byte logging as part of this behavior change.
- Do not change exact scene identity validation.
- Do not change disconnect fade safety behavior.

## Design

Scene recall trigger gating belongs in `SceneRecallFader`, not `Lv1Actor`.

`Lv1Actor` remains the source of LV1 facts. It should continue to mirror the current scene and emit `SceneChanged` when a complete scene identity is observed. Initial connection state is still a real fact and should remain visible to shell state and UI consumers.

`SceneRecallFader` becomes responsible for deciding whether a `SceneChanged` observation is actionable fade automation. Its state is scoped to one LV1 connection generation.

Each spawned recall fader starts in `Priming`:

- The first complete `SceneChanged(index, name)` establishes the generation baseline.
- The baseline event does not fetch LV1 state, run scene recall validation, log fade skip/block messages, start a fade, or finish a fade.
- The fader sets an `arm_after` deadline of `now + 2000 ms`.
- Additional scene observations before `arm_after` update the latest observed baseline and remain non-actionable.

After `arm_after`, the fader transitions to `Armed`. The primed baseline is treated as the last observed identity until the first actionable recall is accepted:

- A scene identity different from the last observed or accepted identity is actionable immediately.
- A scene identity equal to the last observed or accepted identity is actionable only if at least `500 ms` has elapsed since that identity last became the baseline or an accepted recall.
- Identical same-scene observations inside the minimum repeat delay are ignored and do not fetch LV1 state or run recall validation.

Initial arming uses `2000 ms` to give connection and reconnect state sync a conservative settling window before automation can move faders. Same-scene repeat suppression uses a separate `500 ms` minimum delay. The captured duplicate recall burst lasted about `166 ms`, so `500 ms` gives margin while keeping intentional repeat recall behavior available after the app is armed.

## Reconnection

Generation boundaries reset recall trigger state.

On disconnect, existing generation guards make the current recall fader stale. It must not trigger automation after the generation is no longer current.

On reconnect, a new recall fader is spawned for the new generation and starts in `Priming`. The first scene identity after reconnect is baseline only, even if it matches the last scene from the previous connection. Duplicate suppression history is not carried across reconnects.

Active fade disconnect behavior remains owned by existing fade and connection safety paths. Recall trigger gating does not preserve, finish, or restart fades across disconnect.

## Data Flow

```text
LV1 TCP messages
-> Lv1Actor parses and mirrors state
-> Lv1Actor emits SceneChanged facts
-> ShellState/UI can reflect current scene
-> SceneRecallFader applies per-generation trigger gate
-> actionable observations call prepare_scene_recall_fade...
-> FadeEngine receives RecallSceneFade commands
```

## Testing

Add focused tests in `scene_recall_fader`:

- Initial scene observation after fader spawn primes automation and does not send a fade command.
- A later scene observation after the arm delay can send a fade command.
- Duplicate same-scene observations inside the minimum repeat delay send at most one fade command.
- Same-scene repeat after the minimum repeat delay remains actionable.
- Reconnect/new generation starts with fresh priming and does not carry duplicate suppression state from the prior generation.

Use `tokio::time::pause` and `advance` where practical so tests do not depend on real-time sleeps.

## Safety Notes

- Ignored priming and duplicate events must not call recall preparation, because preparation can log skip/block decisions and fetch LV1 state.
- Existing generation checks remain required before starting any fade.
- Existing lockout, stale-state, exact scene identity, disconnect, manual override, abort, and finish-now safety behavior must remain unchanged.

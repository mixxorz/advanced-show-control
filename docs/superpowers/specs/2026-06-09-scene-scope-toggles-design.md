# Scene Scope Toggles Design

## Goal

Separate scene-level fader scope enablement from fade duration.

The app currently treats `duration_ms == 0` as a disabled scene recall fade. That overloads duration with enablement state. After this change, duration only controls timing. A zero duration means an enabled fader scope moves immediately to its stored targets.

## Data Model

Add a per-scene scope toggle object, starting with faders:

```ts
type SceneScopeToggles = {
  faders: boolean;
};
```

Core Rust `SceneConfig`, Tauri show-file structs, Tauri view models, and UI `SceneConfig` types should carry this value as `scope_toggles` in Rust and `scopeToggles` in TypeScript/JSON.

Defaults:

- New stored scene configs get `scope_toggles.faders = true`.
- Existing show files that do not contain `scopeToggles` load with `faders = true`.
- Saved show files write `scopeToggles` for every scene config.

`scoped_channels` remains the list of configured fader targets. `scope_toggles.faders` controls whether those configured targets are active during scene recall.

This lets an engineer disable app-managed fader motion for a scene without losing stored fader targets or scoped-channel choices.

## Recall Behavior

Scene recall validation keeps the current safety order and checks:

- Lockout blocks recall automation.
- LV1 must be connected.
- The fresh LV1 snapshot must exactly match the recalled scene identity.
- A stored scene config must exist.
- Live channel state must be available.
- Scoped channels must exist in the live topology.
- Scoped channels must have stored fader target values.

The scene-scope and duration decisions change as follows:

- If `scope_toggles.faders` is `false`, recall automation skips the scene and emits a visible reason such as `fader scope is disabled`.
- If `scope_toggles.faders` is `true`, the recall policy validates scoped faders and builds fade targets.
- If `duration_ms > 0`, the existing fade engine timing moves faders over that duration.
- If `duration_ms == 0`, recall still starts an enabled fader move. The enabled scoped faders move to their final stored positions immediately.
- Empty scoped targets remain blocked and visible. They are not treated as a successful no-op.

Existing safety behavior must remain unchanged: same-scene overlap, manual override, abort, disconnect handling, generation guards, and exact scene identity validation all stay in force.

## UI And Commands

The Scene tab should expose a scene-level `FADERS` toggle near the duration controls and separate from the scoped-channel grid.

User-facing behavior:

- `FADERS` on: recall moves checked scoped faders to stored targets.
- `FADERS` off: recall does not move faders for that scene, but scoped-channel checkboxes and stored targets remain intact.
- Duration input accepts `0.0` seconds.
- A zero duration is labelled as immediate, not disabled.
- Scene list summaries no longer show `Disabled` for `durationMs == 0`. They should show immediate timing plus fader scope state.

Add a shell command named `set_scene_scope_faders_enabled` for mutating the fader scope toggle. The command must:

- Update show state through the existing show command path.
- Mark the show file dirty only when the value changes.
- Return the updated app snapshot.

Existing per-channel scope commands remain unchanged.

## Storage Compatibility

The `.lv1show` format gains:

```json
"scopeToggles": {
  "faders": true
}
```

When loading older files with no `scopeToggles`, the loader must default to `{ "faders": true }`. This preserves existing configured scene behavior while allowing `durationMs: 0` to take on its new meaning of immediate movement.

No broader migration system is required for this change.

## Tests

Add or update tests in the smallest relevant layers:

- Core show-state tests cover default toggle creation and toggle mutation.
- Show-file mapping tests cover loading old scene configs with missing `scopeToggles` as enabled.
- Recall policy tests cover disabled fader scope skip, enabled zero-duration start, and existing blocked cases.
- Tauri command/projection tests cover fader-toggle updates and dirty-state behavior.
- Frontend typecheck/build covers UI wiring and duration label changes.

## Docs

Update project documentation where it currently says duration `0` disables or skips recall automation.

Mark the matching `IDEAS.md` item complete after implementation.

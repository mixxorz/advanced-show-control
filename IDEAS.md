# Ideas

## New Features

- [ ] Add support for fading pan, balance, and width parameters similar to faders. The app should let scenes scope these parameters explicitly, store their target values, and fade them over the scene duration without changing the existing fader safety model.
- [ ] Add a reconciliation/remapping flow when loading a show file whose stored scene references no longer match the current LV1 scene list. Active scene-list rename/reorder tracking is planned separately, but load-time relinking still needs explicit UX.
- [ ] Auto-reload the last app show file on startup when reconnecting to the same LV1 console. The app should persist enough console identity metadata to avoid loading fade configuration onto the wrong console, and should make any skipped auto-load visible so the user can choose a file manually.
- [ ] Add an event trigger/action automation engine. Users should be able to define automations with triggers and actions, so incoming events can drive configured app behavior without hard-coding every workflow into a dedicated runtime task.

## Bugs

- [x] Scene recall automation can trigger while scenes are being moved around in the LV1 scene list. Investigate whether scene-list edits emit current-scene notifications or otherwise look like recalls, then suppress recall automation for scene-management-only changes without weakening real recall detection.

## Optimization

- [ ] Optimize `AppEventBus` subscriptions so subscribers can receive only the event kinds they care about instead of every published event. Consider typed or filtered subscriptions that reduce noisy data flow while preserving ordering and safety-critical visibility for components that need the full event stream.

## Architecture And Code Quality

- [ ] Tighten Rust data models to make invalid states unrepresentable where practical. For example, stored scene fade targets should not allow a scoped channel without a valid stored fader value unless there is a deliberate migration or validation boundary.

## Completed

- [x] If hardware testing shows one exact final channel send is insufficient, add target verification with safe retry and visible mismatch reporting after fade completion.
- [x] Improve startup and connection UX. The app opens on the connection screen, lets the user choose from auto-discovered LV1 systems, remembers the last connected system in a config file, auto-connects on launch when that system is available, and returns to the connection screen whenever LV1 is disconnected.
- [x] Standardize module type naming and placement. Domain data structures now use `types.rs` consistently in the core runtime modules.
- [x] Add a support log export feature so users can package diagnostic logs when reporting problems. The export should include app logs, recent safety/automation events, version and platform details, and OSC/probe logs similar to the data collected by the probe CLI command, with sensitive show data reviewed or omitted where practical.
- [x] Separate per-scene scope enablement from fade duration. Add scene-level scope toggles, starting with a `FADERS` toggle since the app currently only controls faders. When `FADERS` is enabled, scoped faders should move to their stored targets; when disabled, they should not move. A duration of `0` should mean the enabled faders move to their final positions immediately, not that the scene fade is skipped.

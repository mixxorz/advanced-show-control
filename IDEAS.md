# Ideas

## New Features

- [ ] Add a reconciliation/remapping flow when loading a show file whose stored scene references no longer match the current LV1 scene list.
- [ ] Auto-reload the last app show file on startup when reconnecting to the same LV1 console. The app should persist enough console identity metadata to avoid loading fade configuration onto the wrong console, and should make any skipped auto-load visible so the user can choose a file manually.
- [ ] Add an event trigger/action automation engine. Users should be able to define automations with triggers and actions, so incoming events can drive configured app behavior.

## Bugs

- [ ] Balance/rotation fades can still report manual override false positives during timed fades. Consider limiting override authority to pan control instead of pan, balance, and width.

## Optimization

- [ ] Optimize shell state so that its updates are bounded to at most 25 Hz.

# Status Mode And Safe Design

## Purpose

Clarify the app shell status language so operators can quickly tell whether the app is ready, fading, offline, or intentionally safed.

## Bottom Status Bar

- `Cued` shows the cued scene name when one is selected.
- `Cued` shows `---` when no scene is cued.
- `Current` shows the current LV1 scene name when LV1 reports one.
- `Current` shows `---` when no current scene is available.
- `Mode` shows `Offline` in gray when the app is not connected to LV1.
- `Mode` shows `Safe` in warning/orange when the app lockout state is enabled.
- `Mode` shows `Fading` in warning/orange with a subtle pulse while a fade is running.
- `Mode` shows `Ready` in green when connected, not safed, and not fading.
- Manual override or blocked fade events do not take over the `Mode` label.

Mode priority is:

1. `Offline`
2. `Safe`
3. `Fading`
4. `Ready`

## Top Bar

- Add a fixed-label `SAFE` button near the connection controls.
- The label stays `SAFE` in both states.
- The inactive state uses neutral console button styling.
- The active state uses warning/orange styling so the state is visible without changing the label.
- Clicking the button toggles the existing app lockout state.
- The button remains available regardless of LV1 connection because this is app-side safety state.

## Existing Safety Visibility

Manual override and blocked fade behavior remain visible through logs or other existing safety messaging. They should not be represented as `Mode: Blocked`.

## Tests And Stories

- Cover `Cued` and `Current` fallback display as `---`.
- Cover `Mode: Offline`, `Mode: Safe`, `Mode: Fading`, and `Mode: Ready`.
- Cover the `SAFE` button active and inactive states.
- Add or update Storybook examples for safe and fading states.

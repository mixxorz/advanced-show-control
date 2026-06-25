# Keyboard Shortcut Capture Design

## Context

The Settings tab currently edits keyboard shortcuts as a text key field plus separate modifier checkboxes. That is accurate to the persisted settings model, but it is not how users expect keybinding assignment to work. Users should click a shortcut control, press the desired key combination, and see the assigned shortcut immediately.

Keyboard handling should also be centralized now, before app-wide shortcut execution is wired, so future components do not each add their own `window` or DOM key listeners.

## Goals

- Replace manual shortcut key and modifier controls with a standard keybind capture interaction.
- Capture key and modifier state from one shared app-wide keyboard event source.
- Preserve the existing persisted settings shape and full-object settings replacement command.
- Display shortcuts with OS-appropriate modifier symbols or labels.
- Keep shortcut execution behavior deferred; this change edits settings only.

## Non-Goals

- Do not wire GO or Cue shortcut execution.
- Do not change Rust settings types or settings persistence.
- Do not add multi-stroke shortcuts or shortcut conflict resolution.
- Do not support OS/browser-reserved combinations that never reach the webview.

## User Interaction

The Settings tab shows one row per shortcut action. Each row has the command label and a capture control displaying the current shortcut.

When the user clicks the control, that row enters capture mode and the control displays `Press shortcut...`. The next non-modifier key press records the shortcut with the modifier state present on that event. Pressing `Escape` cancels capture and leaves the existing setting unchanged. Pressing only `Shift`, `Control`, `Alt`, or `Meta` keeps capture mode active and waits for the non-modifier key.

Only one shortcut can be captured at a time. Starting capture for another row replaces the previous pending capture.

## Frontend Architecture

Add a small keyboard layer near the React app root:

- `KeyboardProvider` owns a single app-wide `window.addEventListener("keydown", ...)` listener.
- The provider normalizes browser `KeyboardEvent` objects into a simple app key event shape containing `key`, `shift`, `control`, `alt`, and `meta`.
- The provider exposes shortcut capture state through hooks instead of requiring components to add their own key listeners.
- Capture mode has priority over future normal shortcut dispatch. While capture mode handles an event, it prevents default browser handling and stops propagation.

Settings uses this layer through a shortcut capture hook. `SettingsTab` remains responsible for constructing the full replacement `AppSettings` object and calling `replaceAppSettings`. The capture button remains presentational: it displays the formatted shortcut or the capture prompt and requests capture start on click.

## Data Flow

1. User clicks the GO or Cue shortcut control.
2. Settings starts capture for that action through the keyboard provider.
3. The global keydown listener receives the next key event.
4. Modifier-only keys are ignored while preserving capture mode.
5. `Escape` cancels capture without calling `replaceAppSettings`.
6. Any other key is normalized into the existing `KeyboardShortcut` shape.
7. Settings replaces the full settings object through `replaceAppSettings`.
8. The app projection updates and the control displays the persisted shortcut.

## Shortcut Formatting

Storage remains platform-neutral:

```ts
type KeyboardShortcut = {
  key: string;
  modifiers: {
    shift: boolean;
    control: boolean;
    alt: boolean;
    meta: boolean;
  };
};
```

Formatting is presentation-only. A formatter receives the shortcut and platform, then returns the display string.

On macOS, modifier display uses common symbols:

- `shift`: `⇧`
- `control`: `⌃`
- `alt`: `⌥`
- `meta`: `⌘`

On non-macOS platforms, modifier display uses text labels:

- `shift`: `Shift`
- `control`: `Ctrl`
- `alt`: `Alt`
- `meta`: `Win`

Key labels should be readable and stable for common keys: `Space`, `Enter`, `Escape`, `Tab`, arrow keys, and uppercase single-letter keys. The formatter should be testable with an explicit platform argument rather than depending directly on the test environment.

## Error Handling And Safety

If a key combination is reserved by the OS or webview and no keydown event is delivered, the app cannot capture it. Capture mode stays active until a delivered key records the shortcut or `Escape` cancels.

Settings persistence errors continue to flow through the existing `replaceAppSettings` command behavior. The UI change must not mark show/session data dirty and must not send any LV1 or fade commands.

## Testing

Add frontend tests for:

- GO and Cue capture replacing the full settings object.
- Capturing modifiers from the same keydown event as the non-modifier key.
- Modifier-only keydown not saving or exiting capture mode.
- `Escape` cancelling without saving.
- OS-specific formatting for macOS and non-macOS displays.
- Only the active shortcut row showing capture state.

Run targeted Settings tests first, then frontend typecheck and tests before committing implementation.

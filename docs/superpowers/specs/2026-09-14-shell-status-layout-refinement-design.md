# Shell Status and Layout Refinement Design

## Purpose

Bring the native GPUI shell closer to the approved reference image while preserving the session-menu, connection-dialog, GO, and safety behavior already implemented on this branch.

Reference: `/Users/mixxorz/Downloads/Codex Image Sep 14, 2026, 01_18_47 AM.png`.

## Approved Result

### Navigation

The top navigation contains, in order:

1. session burger menu
2. Scenes
3. Cue Lists
4. Logs
5. Settings

The Events tab and placeholder content are removed for now. This intentionally differs from the reference image, which still shows Events.

### Connection controls

The connection label gains an inline 8 px circular status dot:

- green for Connected;
- amber for Connecting; and
- red for Offline/Disconnected.

The text remains `CONNECTED`, `CONNECTING`, or `OFFLINE`. Both dot and text derive only from the projected `AppConnectionState` snapshot.

The console-name control remains one accessible button that opens the existing connection dialog. A trailing down-chevron communicates that the control opens a chooser; it is not a separate action or a new custom dropdown.

### Bottom status bar

The bottom bar keeps five sections but adopts the reference proportions:

- GO uses approximately 14% of the available width;
- CUED, CURRENT, MODE, and TIME divide the remainder equally;
- all status cells retain consistent padding and vertical separators; and
- the GO button fills most of its section in both dimensions, making it substantially larger than the current intrinsic button.

The GO button retains its existing orange primary treatment and all existing enablement, single-flight, lockout, modal, shortcut-capture, and session-menu guards.

## Architecture and Safety

This is a presentation-only change in the native GPUI shell. It removes no event infrastructure and changes no `AppEventBus`, actor, projector, command, generation, exact-scene, lockout, recall, persistence, or fade behavior.

`MainTab::Events` may be removed because no production behavior routes to it. `AppEventBus` facts and the Logs view remain unchanged.

Connection colors use existing theme tokens: `STATUS_CUED`, `STATUS_WARNING`, and `STATUS_DANGER`. The console chevron uses the GPUI Kit button caret so keyboard activation, disabled behavior, and accessibility remain owned by one control.

Applicable contracts include `production-crate-and-native-host`, `projector-only-frontend-state`, `frontend-safety-is-advisory`, `go-single-flight`, and `cued-scene-resolution`. None is weakened or removed.

## Testing

Use these allowed Rust test styles:

- Pure unit tests for mapping all three projected connection states to label and status color.
- Native GPUI component tests in the existing visual harness for rendered geometry and interaction.

The native harness will verify:

- `tab-Events` is absent and Logs follows Cue Lists;
- the connection dot is 8 px and aligned with its label;
- the console-name button still opens the existing connection dialog;
- GO occupies most of its dedicated section;
- CUED, CURRENT, MODE, and TIME have equal widths; and
- the existing session-menu, modal, GO/Cue, and action-focus checks continue to pass.

Update reviewed visual signatures and inspect every affected PNG. At completion, compare the native ready capture against the reference by navigation, connection indicator, console control, GO prominence, and footer proportions. Record intentional differences, especially removal of Events and differences caused by live app state or platform window chrome.

## Documentation

Update `site/docs/application-shell.md` and its synchronized screenshot so the documented navigation, status dot, console chooser, and bottom bar match the implementation.

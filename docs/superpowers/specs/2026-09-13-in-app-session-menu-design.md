# In-App Session Menu Design

## Purpose

Replace the native session File menu with an in-app burger menu in the GPUI application shell. Keep the current session command paths, file dialogs, validation, and keyboard shortcuts. Add Quit to the in-app menu.

This design implements the focused subset of GitHub issue #50 requested for this change. It does not add recent sessions or dirty-session protection.

## Goals

- Add a square burger-menu button immediately before the Scenes tab.
- Move the current session actions into a GPUI Kit popup menu.
- Add Quit to the popup menu.
- Remove the native File menu on macOS and Windows.
- Retain the standard macOS application menu, including its Quit item.
- Preserve standard session shortcuts and add the platform-standard Quit shortcut.
- Ensure menu actions and shortcuts work immediately after the connection dialog closes.
- Keep existing application commands, file dialogs, validation, error reporting, and backend safety boundaries.

## Non-Goals

- Do not add Open Recent from issue #49.
- Do not add dirty-session Save, Discard, or Cancel prompts from issue #40.
- Do not change startup or offline session-command policy from issue #71.
- Do not change the connection chooser from a modal dialog.
- Do not change show-file persistence, LV1 reconciliation, actor commands, or fade safety behavior.
- Do not add application-menu replacements for macOS platform controls such as About, Services, or Hide.

## User Experience

The application shell will show a square menu button at the far left of the top navigation bar, immediately before Scenes. The button will match the navigation bar height and use the existing chrome colors, border treatment, hover state, and pressed state. A centered icon with three horizontal lines will identify the control. Its accessible name will be `Session menu`.

Activating the button opens a GPUI Kit popup aligned below the button. The menu contains:

1. New Session
2. New from Template…
3. Open Session…
4. separator
5. Save Session
6. Save Session As…
7. separator
8. Quit

Menu items display the keyboard shortcuts registered for their actions. GPUI Kit provides pointer selection, arrow-key navigation, Enter activation, Escape dismissal, click-outside dismissal, menu semantics, and focus restoration.

Only the native File menu is removed. On macOS, the Advanced Show Control application menu remains and continues to include About, Services, Hide, Hide Others, and Quit. Quit therefore appears in both the macOS application menu and the in-app menu. Windows has no native application or File menu after this change.

## Keyboard Shortcuts

The existing session shortcuts remain:

- New Session: Cmd+N on macOS, Ctrl+N on Windows
- Open Session: Cmd+O on macOS, Ctrl+O on Windows
- Save Session: Cmd+S on macOS, Ctrl+S on Windows
- Save Session As: Cmd+Shift+S on macOS, Ctrl+Shift+S on Windows

New from Template does not receive a shortcut.

Quit uses Cmd+Q on macOS and Alt+F4 on Windows. These fixed shortcuts remain reserved from configurable GO and Cue assignments. Shortcut capture continues to take precedence over application actions.

## Architecture

### Menu definition

`app/src/native_ui/menu.rs` remains the owner of session action types and fixed keyboard bindings. It will expose a focused builder for the in-app `PopupMenu`, so item order and action construction are not duplicated in `AppShell`.

The native installation path will:

- bind session and Quit shortcuts on supported platforms;
- install only the standard application menu on macOS; and
- install no native menu on Windows.

The in-app menu will use GPUI Kit's `DropdownMenu` and `PopupMenu` rather than a custom overlay or an OS-native context menu. This preserves the existing design language and delegates generic menu accessibility and interaction behavior to the component library.

### Shell integration

`app/src/native_ui/shell.rs` will render the square burger trigger before the existing tab buttons. Menu-open state is presentation-only state owned by `AppShell`.

The shell will expose whether the session menu is open. `AppRoot` will use that state when routing configurable GO and Cue key presses, preventing those commands from firing while the popup owns interaction. Menu-open state will not be included in the existing session-action modal guard, because popup selections dispatch their action before the popup's dismissal callback completes.

### Command flow

Every popup item dispatches the existing GPUI action type. Those actions continue to terminate at the existing `AppRoot` handlers:

```text
Burger button
  -> GPUI PopupMenu item
  -> existing GPUI action
  -> AppRoot action handler
  -> existing prompt or CommandDispatcher path
  -> existing application command
```

No show-file or LV1 business logic moves into the shell or menu builder. Cancelled file pickers remain no-ops. Invalid paths and command failures continue through the existing notification and command-error flow.

Quit uses one cross-platform `AppRoot` handler. The macOS application-menu Quit item, the in-app Quit item, and the fixed Quit shortcut converge on that handler.

## Focus and Action Availability

The current native File menu can remain unavailable after the startup connection dialog has visibly closed. The startup dialog may open before any application control has focus. GPUI then has no valid previous application focus to restore when it removes the dialog. Native menu validation checks action availability through the current rendered focus path; a stale dialog focus cannot reach the action listeners attached to `AppRoot`. Clicking another application control creates a valid focus path and makes the actions available again.

The implementation will give `AppRoot` a stable, tracked focus handle and establish it before the connection dialog opens. The connection dialog can then restore focus to a live application node when it closes. The popup menu will use the same handle as its explicit action context, ensuring popup actions reach `AppRoot` regardless of transient focus state.

This is a focus-ownership correction, not a delay or retry. Session shortcuts and menu actions must be available immediately after the connection dialog closes, without another click. The connection chooser remains modal while open.

## Interaction and Error Handling

The connection dialog, scene-overwrite confirmation, Cue Lists manager, active file picker, and shortcut-capture UI retain their existing overlap protections. The burger button is not expected to be usable through a modal layer. Once a modal closes, no additional disabled interval is allowed.

While the session popup is open:

- its items remain actionable;
- GO and Cue shortcut routing is suppressed;
- unrelated shell controls cannot receive pointer input through the popup layer; and
- selecting an item closes the popup through GPUI Kit's normal behavior.

The UI remains advisory. Backend LV1 availability, generation, scene identity, reconciliation, persistence, and fade-safety checks stay authoritative under the applicable contracts in `app/src/CONTRACTS`.

Implementation will add or update a local code contract for the focus and routing requirement: after the connection dialog closes, session actions must have a live `AppRoot` action context, and an open session popup must suppress GO/Cue routing without blocking its own session actions.

## Testing

Testing will follow RED-GREEN-REFACTOR and test application behavior rather than GPUI Kit implementation details.

### Native GPUI component tests

Add focused interaction coverage that:

- opens the in-app session menu and proves a selected item reaches the existing `AppRoot` action path;
- opens and closes the startup connection dialog, then proves a session action and fixed shortcut work immediately without an intermediate click; and
- proves GO and Cue routing does not dispatch while the session popup owns interaction.

These tests use real GPUI actions and focus handling. They will not mock internal views or assert Rust source text.

Do not add tests for generic arrow navigation, Enter activation, Escape dismissal, click-outside dismissal, or generic focus restoration. GPUI Kit already owns and tests those behaviors.

### Existing unit tests

Update or replace the existing native-menu composition test instead of adding duplicate coverage. Keep hard-coded expected labels and shortcut values where direct input/output comparison is useful. Update configurable-shortcut conflict coverage for the platform Quit shortcut.

### Visual verification

Update the reviewed native shell snapshots for the square burger button. Capture an open-menu state to review:

- square trigger sizing;
- three-line icon appearance;
- placement immediately before Scenes;
- popup alignment and spacing; and
- consistency with the current dark console theme.

### Commands

Use targeted `cargo nextest` runs during RED-GREEN-REFACTOR. Before completion, run:

```bash
make visual-test
make check
```

No actor test or hardware-smoke coverage is required because the change does not alter actor behavior, LV1 communication, or fader safety logic.

## Documentation Impact

Update architecture or user documentation only where it currently identifies the native File menu as the session-command surface. User-facing screenshots that show the top navigation should be refreshed if they are generated from or intentionally synchronized with the native visual fixtures.

# Native Session Menu Design

## Purpose

Replace the in-app Sessions workflow with normal desktop session-file commands. The app will keep using its current show-file data format, but the released user-facing concept is a session and the saved file extension is `.adsc`.

## Scope

- Remove the `Sessions` top-level tab.
- Add native application `File` menu commands for session file actions.
- Show the current session name and dirty state in the window title bar.
- Rename the app-owned session file extension from `.lv1show` to `.adsc` without changing the file schema or JSON format.
- Keep LV1 connection controls in the app UI.
- Do not add an in-app session dropdown in this scope.

## User Experience

The top tab bar should no longer include `Sessions`. The remaining app navigation should continue to use the existing large, high-contrast console style.

Session file actions should live in the native desktop `File` menu:

- `New Session`
- `Open Session...`
- `Save Session`
- `Save As...`

The title bar should show the current session name so the session is visible without adding another control to the top bar. The dirty marker should match existing dirty-state behavior.

Examples:

- `Advanced Show Control - Untitled *`
- `Advanced Show Control - Tour Prep.adsc`
- `Advanced Show Control - Tour Prep.adsc *`

The top-right LV1 connection control should remain focused on console connection state and system management. Connection workflows such as going offline or switching LV1 systems should continue through the existing connection modal or a later connection-control improvement, not through the File menu.

## Backend Behavior

The existing show/session file schema stays unchanged. Only filenames, dialog filters, default names, backup names, tests, and user-facing copy change from `.lv1show` and show-file terminology to `.adsc` and session terminology where appropriate.

Opening old `.lv1show` files is not part of this scope. The new file dialog filter should prefer `.adsc`. No migration or compatibility code should be added unless a later requirement asks for it.

Save and open behavior remains otherwise unchanged:

- `New Session` clears the current app session state using the existing new-show behavior.
- `Open Session...` uses the existing open workflow and validation behavior.
- `Save Session` saves to the current path, or behaves like `Save As...` when no current path exists.
- `Save As...` prompts for a destination path.
- Listen Mode and safety-related blocks remain unchanged.

## Frontend And Tauri Integration

The React shell should remove `sessions` from the main tab type, tab list, shell rendering, and tests/stories that expect the Sessions tab. The existing `SessionsTab` component and related stories can be deleted if unused after the tab removal.

The Tauri app menu should be implemented in the backend Tauri adapter layer during `build_app` setup. Add a small menu module under `src-tauri/src/ui/` that builds the native `File` menu and handles menu events by command ID.

Menu-triggered commands should not go through React. Instead, extract the existing show-file command adapter bodies into shared async helper functions in `src-tauri/src/ui/commands/show.rs` or a sibling adapter module. Both `#[tauri::command]` functions and native menu event handlers should call those helpers. The helpers should continue to route through `ShowCommand` mailboxes and use the same `rfd` native dialogs. This avoids duplicating business logic while keeping native menu actions available even when focus is not inside the webview.

Native menu command IDs should be stable constants, for example:

- `session:new`
- `session:open`
- `session:save`
- `session:save-as`

Menu event handlers should spawn async work on the Tauri async runtime and log failures through tracing. They do not need to return errors to the menu system. User-visible command failures should still become visible through the existing app log/projection path where the underlying show command already emits state or logs. If a command only fails before reaching show state, such as dialog creation failure, log it with enough context for diagnosis.

The window title should be updated from the React runtime. `AppRuntime` already receives the projected `showFileName`, `showFilePath`, and `showFileDirty` values through `AppViewState`, so it can derive the title without creating a second state source. Add a small title-formatting helper and call the Tauri window API from an effect when those projection fields change. Tests can cover the pure formatting helper without requiring a Tauri window.

If `sessions` appears anywhere in initial tab state, tests, stories, or future persisted UI state, the shell should fall back to `scenes`. The current runtime initializes `activeTab` in memory, so implementation should mainly remove `sessions` from the type and test data.

## Technical Decisions

- Native `File` menu is the canonical location for session actions.
- No in-app session dropdown or top-bar session control will be added.
- React owns title formatting because it already observes projected session state.
- Backend menu handlers own native menu events because Tauri menu events are backend events.
- Shared Rust adapter helpers prevent duplicate show/session command behavior.
- Menu actions and React commands use the same show actor mailbox commands.
- Existing `rfd` dialogs remain the file picker implementation for both menu and command paths.
- `.adsc` is the only preferred extension. `.lv1show` compatibility is out of scope.

## Testing

Add or update tests for:

- Top tab bar no longer rendering `Sessions`.
- Shell tab typing and fallback behavior after removing `sessions`.
- `.adsc` dialog filters/default filenames and backup filename behavior.
- Window title formatting for untitled, saved, and dirty sessions.
- Menu command IDs and menu construction if the app menu logic is testable without launching the full Tauri app.
- Shared show/session command helpers remain used by both Tauri commands and native menu handlers.

Run the relevant frontend and Rust checks after implementation.

## Non-Goals

- No in-app session dropdown.
- No show-file schema change.
- No `.lv1show` migration or compatibility support.
- No redesign of the connection modal.
- No changes to LV1 scene/session handling.

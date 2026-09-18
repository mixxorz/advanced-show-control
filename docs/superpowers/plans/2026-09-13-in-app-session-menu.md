# In-App Session Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the native File menu with a square in-app burger menu that dispatches the existing session actions plus Quit, preserves fixed shortcuts, and removes the stale-focus delay after the connection dialog closes.

**Architecture:** Keep action types, fixed key bindings, and menu construction in `native_ui/menu.rs`; render the trigger from `AppShell`; and continue handling every action in `AppRoot`. Give `AppRoot` one stable tracked focus handle, pass it to the popup as its action context, and expose only presentation-level menu-open state from `AppShell` so GO/Cue routing can be suppressed without blocking the popup's own actions.

**Tech Stack:** Rust, GPUI Kit 0.6.1 `Button`/`DropdownMenu`/`PopupMenu`, GPUI actions and focus handles, `cargo nextest`, native visual snapshots, Zensical Markdown documentation.

## Global Constraints

- Production targets remain macOS 15 or newer and Windows 10 or newer.
- Use GPUI under `app/src/native_ui/`; do not add JavaScript, npm, React, Tauri, or browser-test dependencies.
- Keep New, New from Template, Open, Save, and Save As on their existing `AppRoot` prompt and `CommandDispatcher` paths.
- Keep the macOS application menu, including About, Services, Hide, Hide Others, and Quit; remove only the native File menu.
- Use Cmd+Q on macOS and Alt+F4 on Windows for Quit.
- Do not add Open Recent, dirty-session prompts, or a non-modal connection chooser.
- Do not move LV1 availability, generation, exact-scene, reconciliation, persistence, or fade-safety policy into the UI.
- Use behavior-focused tests; do not duplicate GPUI Kit tests for generic menu navigation or dismissal.
- Follow RED-GREEN-REFACTOR for new application behavior and update existing tests when they already own changed behavior.

---

### Task 1: Replace the native File menu with the complete in-app session menu

**Files:**
- Modify: `app/src/native_ui/menu.rs` — fixed actions and shortcuts, macOS-only native application menu, in-app popup builder and square trigger, focused GPUI interaction coverage.
- Modify: `app/src/native_ui/app.rs` — stable application focus, cross-platform Quit handler, session-popup interaction routing, focus/action-availability regression coverage.
- Modify: `app/src/native_ui/shell.rs` — render the burger trigger before Scenes and own presentation-only popup-open state.
- Modify: `app/src/native_ui/settings_view.rs` — reserve the platform Quit shortcut from GO/Cue capture and update existing conflict tests.
- Modify: `app/src/native_ui/visual.rs` — capture and compare an open-session-menu state and assert immediate post-dialog action availability.
- Modify: `app/src/native_ui/visual_snapshots/*.rgb` — reviewed visual signatures affected by the new top-bar button, including the new open-menu signature.
- Modify: `docs/architecture.md` — describe in-app session actions and the retained native macOS application menu accurately.
- Modify: `site/docs/application-shell.md` — document the burger menu and Quit.
- Modify: `site/docs/reference/keyboard-shortcuts.md` — rename the fixed-menu section and add platform Quit shortcuts.
- Modify: `site/docs/settings.md` — include Quit in reserved shortcuts and replace native File-menu instructions.
- Modify: `site/docs/getting-started.md` — direct users to the session burger menu.
- Modify: `site/docs/troubleshooting.md` — direct open/save troubleshooting to the session burger menu.

**Interfaces:**
- Consumes: existing GPUI actions `NewShow`, `NewShowFromTemplate`, `OpenShow`, `SaveShow`, `SaveShowAs`, and `Quit`; existing `AppRoot` handlers and `CommandDispatcher`; `AppShell::modal_open`; GPUI Kit `DropdownMenu`, `PopupMenu`, `IconName::Menu`, and `FocusHandle`.
- Produces: `MENU_QUIT_SHORTCUT: &str`; `session_menu_button(action_context: FocusHandle, disabled: bool, on_open_change: impl Fn(&bool, &mut Window, &mut App) + 'static) -> impl IntoElement` in `native_ui/menu.rs`; `AppShell::session_menu_open() -> bool`; one live `AppRoot` focus/action context shared with the popup.

- [ ] **Step 1: Add contracts and failing behavior tests**

  Add a declaration-level contract to the `AppRoot` construction/focus path and extend the existing GO shortcut contract. Use this exact obligation, adapting only placement to valid Rust documentation comments:

  ```rust
  /// @cc [owner:mixxorz,label:accessibility;keyboard] session-action-focus
  /// AppRoot MUST establish a live tracked action context before a startup dialog can open. After
  /// that dialog closes, fixed session and Quit shortcuts and in-app session-menu actions MUST
  /// reach AppRoot without requiring another pointer or focus event.
  ```

  Extend `go-shortcut-routing` so key events owned by an open session menu MUST NOT dispatch GO or Cue.

  Replace the existing native `file_menu_exposes_new_from_template` test with a GPUI Kit interaction test modeled on GPUI Kit's own `tests/menu.rs`. Use a real focus handle and real action listener, click `session-menu`, inspect the hard-coded visible labels at popup indices `0`, `1`, `2`, `4`, `5`, and `7`, click `New Session`, and assert that the listener rendered an observable `new-selected` marker:

  ```rust
  assert_eq!(window.within("popup-menu").find(0usize).label().as_deref(), Some("New Session"));
  assert_eq!(window.within("popup-menu").find(1usize).label().as_deref(), Some("New from Template…"));
  assert_eq!(window.within("popup-menu").find(2usize).label().as_deref(), Some("Open Session…"));
  assert_eq!(window.within("popup-menu").find(4usize).label().as_deref(), Some("Save Session"));
  assert_eq!(window.within("popup-menu").find(5usize).label().as_deref(), Some("Save Session As…"));
  assert_eq!(window.within("popup-menu").find(7usize).label().as_deref(), Some("Quit"));
  ```

  In the existing `native_ui/visual.rs` AppRoot harness, add the focus regression before any synthetic click after the connected snapshot closes the startup dialog:

  ```rust
  assert!(window.is_action_available(&NewShow, cx));
  assert!(window.is_action_available(&Quit, cx));
  ```

  This must exercise the real `AppRoot`, real startup dialog, and real GPUI action availability; do not replace it with a helper-only unit test.

  Update the existing shortcut-conflict tests with hard-coded platform expectations:

  ```rust
  #[cfg(not(target_os = "macos"))]
  assert_eq!(
      shortcut_conflict_label(ShortcutAction::Go, &fixed_quit_shortcut(), &AppSettings::default()),
      Some("Quit Advanced Show Control")
  );
  ```

  On macOS, retain the current Cmd+Q expectation while avoiding duplicate Quit entries in `fixed_shortcut_conflicts()`.

- [ ] **Step 2: Run the focused tests and verify RED**

  Run:

  ```bash
  cargo nextest run -p advanced-show-control native_ui::menu::tests
  cargo nextest run -p advanced-show-control native_ui::settings_view::tests
  make visual-test
  ```

  Expected: the new menu test fails to compile because the in-app trigger and popup builder do not exist; the settings test fails because non-macOS Quit is not yet reserved; and the visual test fails its new action-availability assertion because `AppRoot` has no stable tracked focus. Fix only test setup errors until failures identify missing behavior.

- [ ] **Step 3: Implement the menu, shortcut, and focus path minimally**

  In `native_ui/menu.rs`:

  - Keep every action type, but make `Quit` and its handler cross-platform.
  - Define `MENU_QUIT_SHORTCUT` as `cmd-q` on macOS and `alt-f4` otherwise.
  - Bind New, Open, Save, Save As, and Quit in the global key-binding list.
  - Keep macOS Hide and Hide Others bindings.
  - Change macOS `set_menus` to install only `application_menu()` and remove the non-macOS `set_menus` call.
  - Delete `file_menu()`.
  - Build the in-app menu with the exact approved order and existing actions.
  - Build the trigger with `bordered_button("session-menu")`, `IconName::Menu`, the accessible label `Session menu`, equal padding on all sides so the icon-only control is square, and GPUI Kit's `DropdownMenu`.
  - Set the popup's `action_context` to the stable `AppRoot` focus handle so shortcut labels resolve and action dispatch does not depend on transient popup/dialog focus.
  - Accept an `on_open_change` callback and a `disabled` value rather than exposing GPUI keyed state to callers.

  The core builder should retain this direct shape rather than introducing a new menu model:

  ```rust
  menu
      .action_context(action_context.clone())
      .menu("New Session", Box::new(NewShow))
      .menu("New from Template…", Box::new(NewShowFromTemplate))
      .menu("Open Session…", Box::new(OpenShow))
      .separator()
      .menu("Save Session", Box::new(SaveShow))
      .menu("Save Session As…", Box::new(SaveShowAs))
      .separator()
      .menu("Quit", Box::new(Quit))
  ```

  In `native_ui/app.rs`:

  - Add `focus: FocusHandle` to `AppRoot`.
  - Create and focus it during `AppRoot::new` before asynchronous startup can open the connection dialog.
  - Track it on the root div that owns the existing action listeners.
  - Pass a clone to `AppShell::new` for popup `action_context`.
  - Import `Quit` on all platforms, keep only Hide and Hide Others behind the macOS cfg, and register `on_quit` on all platforms.
  - Preserve shortcut-capture precedence in `on_quit`.
  - Keep custom modal Escape handling separate from popup-open handling.
  - Add `self.shell.read(cx).session_menu_open()` to the `InteractionState.modal_open` expression used by `route_action`; do not add it to `action_blocked()`, because `PopupMenu` dispatches an item action before completing dismissal.

  In `native_ui/shell.rs`:

  - Store the passed action-context focus handle and `session_menu_open: bool`.
  - Expose `pub fn session_menu_open(&self) -> bool`.
  - Render the menu trigger as the first child of the left navigation group, directly before the Scenes tab.
  - Update open state through the trigger's `on_open_change` callback and call `cx.notify()`.
  - Include menu-open state when disabling GO and top-bar show controls, but do not classify it as a custom modal or route Escape into `dismiss_modal()`.

  In `native_ui/settings_view.rs`, add one `fixed_quit_shortcut()` value to `fixed_shortcut_conflicts()`. It must be Cmd+Q on macOS and Alt+F4 on Windows/non-macOS. Keep the label `Quit Advanced Show Control` and avoid adding Cmd+Q twice on macOS.

- [ ] **Step 4: Run focused tests and refactor while green**

  Run:

  ```bash
  cargo nextest run -p advanced-show-control native_ui::menu::tests
  cargo nextest run -p advanced-show-control native_ui::settings_view::tests
  cargo nextest run -p advanced-show-control native_ui::keyboard::tests
  ```

  Expected: PASS with no warnings. Then refactor duplicated platform modifier construction or menu assembly only if the passing implementation exposes duplication. Keep the public surface limited to the trigger/builder, shortcut constants already consumed elsewhere, and `AppShell::session_menu_open()`.

- [ ] **Step 5: Add and review native visual coverage**

  In `native_ui/visual.rs`, after the startup dialog has closed and the ready snapshot is rendered:

  - assert `NewShow` and `Quit` are immediately available before any synthetic click;
  - click `session-menu`;
  - render a frame and assert `popup-menu` exists;
  - capture `native-session-menu.png`;
  - include it in dimension/distinctness checks and the perceptual-signature list; and
  - dismiss the popup before continuing existing tab and SAFE-state captures.

  Run:

  ```bash
  make visual-test
  ```

  Expected before signature update: FAIL only because the new trigger changes existing reviewed signatures and `native-session-menu.rgb` does not exist.

  Generate candidates:

  ```bash
  make visual-update
  ```

  Inspect every PNG in `dist/visual/` affected by the top bar. Confirm that the trigger is square, has three centered horizontal lines, sits immediately before Scenes, and that the popup is aligned and readable. Only after inspection, keep the updated `.rgb` signatures and rerun:

  ```bash
  make visual-test
  ```

  Expected: PASS.

- [ ] **Step 6: Update current architecture and user documentation**

  Make direct terminology changes:

  - In `docs/architecture.md`, replace claims that session commands originate from native menus with the in-app session menu plus retained macOS application-menu distinction.
  - In `site/docs/application-shell.md`, describe the square burger button before Scenes, list all menu commands including New from Template and Quit, and include platform-specific Quit shortcuts.
  - In `site/docs/reference/keyboard-shortcuts.md`, rename `Fixed file shortcuts` to `Fixed application shortcuts` and add Cmd+Q/Alt+F4 for Quit.
  - In `site/docs/settings.md`, include Quit in the fixed conflicts and replace `File > Save Session` with the session-menu instruction.
  - In `site/docs/getting-started.md`, replace `File >` steps with opening the burger menu and selecting the named command.
  - In `site/docs/troubleshooting.md`, replace `File > Open Session...` with the burger-menu path.

  Do not rewrite historical specs or plans. Do not add Open Recent or dirty-session claims.

- [ ] **Step 7: Validate contracts, formatting, and the complete change**

  Run contract discovery manually for every changed declaration and, if `cc-check` is available, run:

  ```bash
  cc-check list app/src/native_ui/menu.rs
  cc-check list app/src/native_ui/app.rs
  cc-check list app/src/native_ui/shell.rs
  cc-check format app/src/native_ui/menu.rs
  cc-check format app/src/native_ui/app.rs
  cc-check format app/src/native_ui/shell.rs
  ```

  If the executable remains unavailable, record that limitation and manually verify `app/src/CONTRACTS`, `session-action-focus`, and `go-shortcut-routing` against implementation and tests.

  Then run:

  ```bash
  cargo fmt --all -- --check
  make check
  make visual-test
  ```

  Expected: all commands PASS with no warnings or snapshot mismatches.

- [ ] **Step 8: Review and commit the single deliverable**

  Inspect only intended files:

  ```bash
  git status --short
  git diff --check
  git diff -- app/src/native_ui/menu.rs app/src/native_ui/app.rs app/src/native_ui/shell.rs app/src/native_ui/settings_view.rs app/src/native_ui/visual.rs docs/architecture.md site/docs
  git diff --stat
  ```

  Confirm the diff contains no Open Recent implementation, dirty-session guard, actor changes, or unrelated refactor. Stage the intended source, contracts, docs, and reviewed visual signatures, then commit:

  ```bash
  git add app/src/native_ui/menu.rs \
    app/src/native_ui/app.rs \
    app/src/native_ui/shell.rs \
    app/src/native_ui/settings_view.rs \
    app/src/native_ui/visual.rs \
    app/src/native_ui/visual_snapshots \
    docs/architecture.md \
    site/docs/application-shell.md \
    site/docs/reference/keyboard-shortcuts.md \
    site/docs/settings.md \
    site/docs/getting-started.md \
    site/docs/troubleshooting.md
  git commit -m "feat: add in-app session menu"
  ```

  Expected: commit succeeds with hooks enabled and the worktree is clean.

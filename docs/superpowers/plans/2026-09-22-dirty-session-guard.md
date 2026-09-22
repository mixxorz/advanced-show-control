# Dirty Session Guard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent dirty session loss across close, New, New from Template, Open, and Quit while preserving existing save and picker behavior.

**Architecture:** Add a pure presentation orchestration state machine and let `AppRoot` adapt its effects to GPUI prompts, path pickers, serial application commands, and quit. Backend Show state remains authoritative through projected snapshots, and no LV1/Fade path changes.

**Tech Stack:** Rust, GPUI Kit, Tokio command dispatcher, cargo-nextest.

## Global Constraints

- Native Rust GPUI Kit only; no Tauri, React, JavaScript, or npm dependencies.
- Never block the GPUI thread on actor replies, file I/O, or path prompts.
- Pending navigation is presentation-only; dirty state and file path remain backend-owned/projected.
- The feature must not recall scenes, alter active fades, or send LV1/fader commands.
- Use pure unit tests and native GPUI tests; no source-string tests or private actor-state inspection.

---

### Task 1: Pure destructive-action coordinator

**Files:**
- Create: `app/src/native_ui/session_guard.rs`
- Modify: `app/src/native_ui/mod.rs`
- Test: inline `app/src/native_ui/session_guard.rs`

**Interfaces:**
- Consumes: projected `dirty: bool`, `titled: bool`, prompt decisions, save destination outcomes, and command completion IDs/results.
- Produces: `SessionGuard::request`, `SessionGuard::choose`, `SessionGuard::save_destination`, and `SessionGuard::command_finished`, each returning a typed `GuardEffect`.

- [ ] **Step 1: Write failing pure unit tests**

Cover clean direct continuation; dirty prompt; Discard and Cancel; titled Save waiting for command success; untitled Save requesting a destination; Save As cancellation; save failure; and ignoring a second destructive request while pending.

```rust
assert_eq!(guard.request(SessionAction::New, true), GuardEffect::Prompt);
assert_eq!(guard.choose(GuardChoice::Save, false), GuardEffect::ChooseSaveDestination);
assert_eq!(guard.save_destination(None), GuardEffect::None);
assert!(!guard.is_pending());
```

- [ ] **Step 2: Verify RED**

Run: `cargo nextest run -p advanced-show-control session_guard`
Expected: compilation/test failure because `SessionGuard` and its transitions do not exist.

- [ ] **Step 3: Implement the minimal coordinator and contract**

Use enums equivalent to:

```rust
pub(super) enum SessionAction { New, NewFromTemplate, Open, Quit }
pub(super) enum GuardChoice { Save, Discard, Cancel }
pub(super) enum GuardEffect {
    None, Prompt, ChooseSaveDestination, SaveCurrent(SessionAction),
    SaveTo(std::path::PathBuf, SessionAction), Continue(SessionAction),
}
```

Store at most one action and optional save command ID. Add an `@cc` contract requiring continuation only after clean admission, discard, or successful save and requiring every cancellation/failure to clear pending intent.

- [ ] **Step 4: Verify GREEN**

Run: `cargo nextest run -p advanced-show-control session_guard`
Expected: all coordinator tests pass.

### Task 2: Integrate all GPUI action and close routes

**Files:**
- Modify: `app/src/native_ui/app.rs`
- Modify: `app/src/native_ui/entry.rs` only if app-level close registration cannot be installed from `AppRoot`
- Test: inline native GPUI tests in `app/src/native_ui/app.rs`

**Interfaces:**
- Consumes: Task 1 `SessionGuard` effects, existing `CommandDispatcher`, projected `show_file_dirty/show_file_path`, GPUI native prompt/path APIs.
- Produces: one guarded request path used by New, New from Template, Open, Quit, and native window close.

- [ ] **Step 1: Write failing native/pure adapter tests**

Register `Window::on_window_should_close` in a GPUI test and assert a dirty close returns false and requests the guard while a clean close returns true. Add focused adapter tests proving Cancel does not dispatch, Discard continues, save failure does not continue, and a successful save does.

- [ ] **Step 2: Verify RED**

Run: `cargo nextest run -p advanced-show-control dirty_session`
Expected: failure because close interception and guarded action routing are absent.

- [ ] **Step 3: Implement GPUI adaptation**

Add one `SessionGuard` field to `AppRoot`. Route New/New from Template/Open/Quit handlers through `request_session_action`. Present:

```rust
window.prompt(
    PromptLevel::Warning,
    "Save changes before continuing?",
    Some("Your unsaved session changes will be lost if you discard them."),
    &["Save", "Discard", "Cancel"],
    cx,
)
```

Await prompt and path receivers with `cx.spawn`. Dispatch existing serial save commands and attach returned command IDs to the guard. In `UiEvent::CommandFinished`, advance only the matching guarded save; success continues and error clears intent. Keep existing notification behavior. Register the window close callback against the `AppRoot` weak entity; clean close returns true, dirty close returns false and requests guarded Quit. Add an `@cc` contract for close veto and shared guard routing.

- [ ] **Step 4: Verify GREEN**

Run: `cargo nextest run -p advanced-show-control dirty_session`
Expected: all dirty-session integration tests pass.

- [ ] **Step 5: Run related native UI tests**

Run: `cargo nextest run -p advanced-show-control native_ui::app native_ui::menu`
Expected: existing save/menu tests and new guard tests pass.

### Task 3: Align architecture and user documentation

**Files:**
- Modify: `docs/architecture.md`
- Modify: `site/docs/application-shell.md`
- Modify: `site/docs/troubleshooting.md`

**Interfaces:**
- Consumes: implemented prompt behavior and exact labels.
- Produces: accurate architecture ownership and user instructions.

- [ ] **Step 1: Update docs**

State that native UI owns one pending destructive action; projected Show state decides whether the guard is needed; save failure/cancellation cancels continuation. Replace the version-2 warning with Save/Discard/Cancel instructions and clarify Save As cancellation in troubleshooting.

- [ ] **Step 2: Validate prose and links**

Run: `git diff --check`
Expected: exit 0.

### Task 4: Final focused verification and commit

**Files:** all files above.

**Interfaces:** none.

- [ ] **Step 1: Validate contract syntax manually and with tooling when available**

Run: `cc-check format app/src/native_ui/session_guard.rs app/src/native_ui/app.rs`
Expected: exit 0. If `cc-check` is unavailable, inspect grammar manually and report that limitation.

- [ ] **Step 2: Format and test**

Run:

```bash
cargo fmt --all -- --check
cargo nextest run -p advanced-show-control session_guard
cargo nextest run -p advanced-show-control native_ui::app native_ui::menu
make visual-test
```

Expected: all commands exit 0. If the visual runner cannot execute in the environment, report the exact failure and retain manual prompt checks as a remaining risk.

- [ ] **Step 3: Review and commit**

```bash
git status --short
git diff --check
git diff
git add app/src/native_ui/session_guard.rs app/src/native_ui/mod.rs app/src/native_ui/app.rs app/src/native_ui/entry.rs docs/architecture.md site/docs/application-shell.md site/docs/troubleshooting.md docs/superpowers/plans/2026-09-22-dirty-session-guard.md
git commit -m "feat: guard dirty session actions"
```

Stage only paths actually changed; do not amend or bypass hooks.

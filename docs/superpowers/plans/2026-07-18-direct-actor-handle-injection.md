# Direct App-Lifetime Actor Handle Injection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete issue #58 by injecting managed Show and settings actor handles directly into Tauri adapters, menus, and debug commands.

**Architecture:** Tauri already manages `ShowStateHandle` and `SettingsHandle` as app-lifetime state. Consumers obtain those exact handles directly while `AppLifecycle` keeps private clones only for runtime orchestration.

**Tech Stack:** Rust 2024, Tauri 2 managed state, Tokio oneshot mailboxes, cargo-nextest

## Global Constraints

- Execute after issue #59 and before issue #56.
- Preserve all Tauri command names, frontend arguments, reply types, mailbox commands, and error mapping.
- Actor handles remain dumb senders; no domain convenience methods or adapter business logic may be added.
- Keep generation-scoped LV1/fade/scenes access on `AppLifecycle`.
- Before advancing to issue #56, run one successful `make smoke` and inspect `logs/debug-smoke-report.txt`.

---

### Task 1: Replace Lifecycle Pass-Throughs With Managed State

**Files:**
- Modify: `src-tauri/src/ui/commands/show.rs:1-160`
- Modify: `src-tauri/src/ui/commands/settings.rs:1-24`
- Modify: `src-tauri/src/ui/menu.rs:1-228`
- Modify: `src-tauri/src/ui/debug/commands.rs:1-229`
- Modify: `src-tauri/src/lifecycle/mod.rs:210-212,561-563`
- Verify: `src-tauri/src/ui/mod.rs:21-87`
- Verify: `src-tauri/src/ui/debug.rs:12-72`

**Interfaces:**
- Consumes: Tauri-managed `ShowStateHandle` and `SettingsHandle` already registered by production and debug setup.
- Produces: direct `State<'_, ShowStateHandle>` / `State<'_, SettingsHandle>` adapters and direct `AppHandle::state::<ShowStateHandle>()` menu/debug access; removes `AppLifecycle::current_show/current_settings`.

- [ ] **Step 1: Record the current targeted baseline**

Run:

```bash
cargo nextest run -p advanced-show-control commands::tests
cargo nextest run -p advanced-show-control ui::menu::tests
cargo nextest run -p advanced-show-control ui::tests
```

Expected: all current adapter/menu symbol tests pass.

- [ ] **Step 2: Create a compile-red checkpoint by removing pass-through methods**

Delete only:

```rust
pub async fn current_settings(&self) -> SettingsHandle {
    self.settings.clone()
}

pub async fn current_show(&self) -> ShowStateHandle {
    self.show.clone()
}
```

Run `cargo check -p advanced-show-control`. Expected: compile errors identify six Show adapters, one settings adapter, four menu actions, and two debug Show-file commands. Do not commit this red state.

- [ ] **Step 3: Inject `ShowStateHandle` into all Show command adapters**

In `ui/commands/show.rs`, remove the lifecycle import, import `ShowStateHandle`, change each first parameter to `show: State<'_, ShowStateHandle>`, and delete each `let show = lifecycle.current_show().await;`. Apply this exact pattern to `refresh_lv1_discovery`, `new_show_file`, `open_show_file_dialog`, `save_show_file`, `save_show_file_as_dialog`, and `set_lockout`:

```rust
#[tauri::command]
pub async fn refresh_lv1_discovery(
    show: State<'_, ShowStateHandle>,
    timeout_ms: Option<u64>,
) -> Result<ShowCommandResult, String> {
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::RefreshLv1Discovery {
        timeout_ms,
        reply: Some(reply),
    })
    .await
    .map_err(|_| AppCommandError::ShowUnavailable)
    .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}
```

Keep every existing command body after handle acquisition unchanged.

- [ ] **Step 4: Inject `SettingsHandle` into the settings adapter**

Replace `replace_app_settings` with:

```rust
#[tauri::command]
pub async fn replace_app_settings(
    settings_handle: State<'_, SettingsHandle>,
    settings: AppSettings,
) -> Result<SettingsCommandResult, String> {
    let (reply, rx) = oneshot::channel();
    settings_handle
        .send(SettingsCommand::ReplaceSettings { settings, reply })
        .await
        .map_err(|_| AppCommandError::CommandFailed("Settings unavailable".to_string()))
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}
```

Remove the lifecycle import and import `SettingsHandle` through `crate::settings`.

- [ ] **Step 5: Retrieve managed Show state in menu actions**

Remove the lifecycle import, import `ShowStateHandle`, and replace lifecycle lookup in `new_session_from_menu`, `open_session_from_menu`, `save_session_from_menu`, and `save_session_as_from_menu` with:

```rust
let show = app.state::<ShowStateHandle>().inner().clone();
```

Keep `tauri::Manager` imported and keep each existing explicit `ShowCommand`/oneshot body unchanged.

- [ ] **Step 6: Retrieve managed Show state in debug Show-file commands**

Keep `State<AppLifecycle>` on the three generation-scoped LV1 debug commands. For only `debug_smoke_load_scene_settings_session` and `debug_smoke_load_unlinked_scene_session`:

```rust
pub async fn debug_smoke_load_scene_settings_session<R: Runtime>(
    app: AppHandle<R>,
) -> Result<(), String>
```

```rust
pub async fn debug_smoke_load_unlinked_scene_session<R: Runtime>(
    app: AppHandle<R>,
) -> Result<String, String>
```

Import `ShowStateHandle` and `tauri::Manager`, then replace each lifecycle lookup with:

```rust
let show = app.state::<ShowStateHandle>().inner().clone();
```

Do not change debug command registration or file-loading behavior.

- [ ] **Step 7: Compile and run targeted verification**

Run:

```bash
cargo check -p advanced-show-control
cargo nextest run -p advanced-show-control commands::tests
cargo nextest run -p advanced-show-control ui::menu::tests
cargo nextest run -p advanced-show-control ui::tests
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
```

Expected: production and debug builders compile with direct exact managed types; all command symbols remain stable.

- [ ] **Step 8: Run the issue smoke checkpoint**

Run `make smoke`, then read `logs/debug-smoke-report.txt`. Expected: authoritative suite success, including the debug Show-file commands.

- [ ] **Step 9: Commit the verified issue**

```bash
git status --short
git diff -- src-tauri/src/ui src-tauri/src/lifecycle/mod.rs
git add src-tauri/src/ui/commands/show.rs src-tauri/src/ui/commands/settings.rs src-tauri/src/ui/menu.rs src-tauri/src/ui/debug/commands.rs src-tauri/src/lifecycle/mod.rs
git commit -m "refactor: inject app-lifetime actor handles"
```

# Lifecycle And UI Boundary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce the app lifecycle owner seam and move Tauri adapter setup into `ui/` while preserving current command behavior and frontend contracts.

**Architecture:** This plan implements phases 5-6 of `docs/superpowers/specs/2026-06-19-single-crate-command-projection-architecture-design.md`. The lifecycle owner replaces direct Tauri ownership of the current command bus holder first, then runtime lifecycle helper methods move behind it without changing direct emits, command-return snapshots, `ShellState`, or `ActiveCommandBus` semantics yet. The `ui/` module becomes the Tauri adapter entry point for setup and command registration, but existing command functions remain behavior-compatible.

**Tech Stack:** Rust 2024, Tokio, Tauri 2, cargo-nextest, existing `ShellState`, existing `ActiveCommandBus`, existing `AppCommandBus`, existing Tauri command functions.

## Global Constraints

- Follow the approved architecture spec phases 5-6 only.
- Do not change LV1 protocol behavior.
- Do not weaken lockout, exact scene identity validation, generation guards, disconnect behavior, manual override, abort, or overlap safety behavior.
- Do not route logs through `AppEventBus`.
- Do not remove `ShellState`, `ActiveCommandBus`, direct emits, or command-return snapshots in this plan.
- Preserve existing Tauri command names and frontend command return payloads.
- Preserve `app-status-changed` emission behavior until the projector-only phase.
- Preserve `lv1-probe` as a supported binary.
- Each task must leave the app working.

---

## File Structure

- Modify: `src-tauri/src/lifecycle/mod.rs`
  - Owns the current runtime lifecycle seam for now.
  - Provides `AppLifecycle` as the managed Tauri state object that wraps the current `ActiveCommandBus`.
  - Later tasks in this plan move runtime handle operations behind this API.
- Modify: `src-tauri/src/app_state/shell.rs`
  - Stops importing `ActiveCommandBus` from `commands.rs` once the holder moves to `lifecycle/`.
  - Keeps `RuntimeHandles` and generation-sensitive state methods in place for this phase.
- Modify: `src-tauri/src/commands.rs`
  - Keeps existing Tauri command functions and return values.
  - Switches runtime lifecycle access from `State<ActiveCommandBus>` to `State<AppLifecycle>`.
  - Delegates runtime install/clear/current-bus operations to lifecycle methods.
- Modify: `src-tauri/src/ui/mod.rs`
  - Owns Tauri adapter setup and command registration.
  - Re-exports or installs the existing command handler list without changing command names.
- Modify: `src-tauri/src/main.rs`
  - Becomes a thin app entry point that calls `ui::build_app()`.
- Tests: existing tests in `src-tauri/src/commands.rs`, `src-tauri/src/app_state/shell.rs`, and new tests in `src-tauri/src/lifecycle/mod.rs` and `src-tauri/src/ui/mod.rs`.

---

### Task 1: Move Current Command Bus Holder Into Lifecycle Module

**Files:**
- Modify: `src-tauri/src/lifecycle/mod.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: current `commands::ActiveCommandBus` behavior: `set(Option<AppCommandBus>)` and `current() -> Option<AppCommandBus>`.
- Produces: `crate::lifecycle::ActiveCommandBus` with the same methods and semantics.

- [ ] **Step 1: Write lifecycle holder tests**

Add this test module to `src-tauri/src/lifecycle/mod.rs` below the module docs:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::events::AppEventBus;

    #[tokio::test]
    async fn active_command_bus_tracks_current_bus() {
        let holder = ActiveCommandBus::default();
        assert!(holder.current().await.is_none());

        let command_bus = AppCommandBus::new(AppEventBus::default());
        holder.set(Some(command_bus.clone())).await;
        assert!(holder.current().await.is_some());

        holder.set(None).await;
        assert!(holder.current().await.is_none());
    }
}
```

- [ ] **Step 2: Run lifecycle holder test and verify failure**

Run: `cargo nextest run -p advanced-show-control lifecycle::tests::active_command_bus_tracks_current_bus`

Expected: FAIL because `lifecycle::ActiveCommandBus` does not exist yet.

- [ ] **Step 3: Move the holder implementation into `lifecycle/mod.rs`**

Add this code above the test module in `src-tauri/src/lifecycle/mod.rs`:

```rust
use crate::runtime::commands::AppCommandBus;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
pub struct ActiveCommandBus(pub Arc<Mutex<Option<AppCommandBus>>>);

impl ActiveCommandBus {
    pub async fn set(&self, command_bus: Option<AppCommandBus>) {
        *self.0.lock().await = command_bus;
    }

    pub async fn current(&self) -> Option<AppCommandBus> {
        self.0.lock().await.clone()
    }
}
```

- [ ] **Step 4: Remove duplicate holder from `commands.rs`**

In `src-tauri/src/commands.rs`, delete the current `ActiveCommandBus` definition and its direct imports:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;
```

Then add this import near the other crate imports:

```rust
use crate::lifecycle::ActiveCommandBus;
```

- [ ] **Step 5: Update shell and main imports**

In `src-tauri/src/app_state/shell.rs`, replace:

```rust
use crate::commands::ActiveCommandBus;
```

with:

```rust
use crate::lifecycle::ActiveCommandBus;
```

In `src-tauri/src/main.rs`, replace:

```rust
use advanced_show_control::commands::ActiveCommandBus;
```

with:

```rust
use advanced_show_control::lifecycle::ActiveCommandBus;
```

- [ ] **Step 6: Update tests that construct the holder**

In `src-tauri/src/commands.rs` and `src-tauri/src/app_state/shell.rs`, replace test references to:

```rust
crate::commands::ActiveCommandBus::default()
```

with:

```rust
crate::lifecycle::ActiveCommandBus::default()
```

Keep unqualified `ActiveCommandBus::default()` where the module import already resolves to `crate::lifecycle::ActiveCommandBus`.

- [ ] **Step 7: Run command and shell holder regression tests**

Run: `cargo nextest run -p advanced-show-control active_command_bus_tracks_current_bus stale_runtime_handle_installation_is_rejected replacement_connect_cleanup_aborts_existing_runtime_and_clears_command_bus`

Expected: PASS.

- [ ] **Step 8: Commit holder move**

Run: `git status --short`

Then commit:

```bash
git add src-tauri/src/lifecycle/mod.rs src-tauri/src/app_state/shell.rs src-tauri/src/commands.rs src-tauri/src/main.rs
git commit -m "refactor: move command bus holder to lifecycle"
```

---

### Task 2: Introduce `AppLifecycle` As Managed Runtime Boundary

**Files:**
- Modify: `src-tauri/src/lifecycle/mod.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `ActiveCommandBus` from Task 1.
- Produces:
  - `pub struct AppLifecycle`
  - `impl Default for AppLifecycle`
  - `pub fn command_bus_holder(&self) -> ActiveCommandBus`
  - `pub async fn set_command_bus(&self, command_bus: Option<AppCommandBus>)`
  - `pub async fn current_command_bus(&self) -> Option<AppCommandBus>`

- [ ] **Step 1: Write `AppLifecycle` tests**

Add these tests to the existing `#[cfg(test)] mod tests` in `src-tauri/src/lifecycle/mod.rs`:

```rust
#[tokio::test]
async fn app_lifecycle_exposes_current_command_bus() {
    let lifecycle = AppLifecycle::default();
    assert!(lifecycle.current_command_bus().await.is_none());

    let command_bus = AppCommandBus::new(AppEventBus::default());
    lifecycle.set_command_bus(Some(command_bus)).await;
    assert!(lifecycle.current_command_bus().await.is_some());
}

#[tokio::test]
async fn app_lifecycle_command_bus_holder_is_shared() {
    let lifecycle = AppLifecycle::default();
    let holder = lifecycle.command_bus_holder();

    let command_bus = AppCommandBus::new(AppEventBus::default());
    holder.set(Some(command_bus)).await;

    assert!(lifecycle.current_command_bus().await.is_some());
}
```

- [ ] **Step 2: Run the new tests and verify failure**

Run: `cargo nextest run -p advanced-show-control lifecycle::tests::app_lifecycle_`

Expected: FAIL because `AppLifecycle` does not exist yet.

- [ ] **Step 3: Implement minimal `AppLifecycle`**

Add this code to `src-tauri/src/lifecycle/mod.rs` after `ActiveCommandBus`:

```rust
#[derive(Clone, Default)]
pub struct AppLifecycle {
    command_bus: ActiveCommandBus,
}

impl AppLifecycle {
    pub fn command_bus_holder(&self) -> ActiveCommandBus {
        self.command_bus.clone()
    }

    pub async fn set_command_bus(&self, command_bus: Option<AppCommandBus>) {
        self.command_bus.set(command_bus).await;
    }

    pub async fn current_command_bus(&self) -> Option<AppCommandBus> {
        self.command_bus.current().await
    }
}
```

- [ ] **Step 4: Manage `AppLifecycle` instead of `ActiveCommandBus` in main**

In `src-tauri/src/main.rs`, replace:

```rust
use advanced_show_control::lifecycle::ActiveCommandBus;
```

with:

```rust
use advanced_show_control::lifecycle::AppLifecycle;
```

Replace:

```rust
.manage(ActiveCommandBus::default())
```

with:

```rust
.manage(AppLifecycle::default())
```

- [ ] **Step 5: Switch command state parameters to `AppLifecycle`**

In `src-tauri/src/commands.rs`, replace command parameters of this form:

```rust
active_command_bus: State<'_, ActiveCommandBus>,
```

with:

```rust
lifecycle: State<'_, crate::lifecycle::AppLifecycle>,
```

Inside each affected command, create the existing holder before calling current helpers:

```rust
let active_command_bus = lifecycle.command_bus_holder();
```

For `abort_all_fades`, use the lifecycle directly:

```rust
let command_bus = lifecycle.current_command_bus().await;
let command_bus = command_bus.ok_or_else(|| "Fade runtime is unavailable".to_string())?;
command_bus
    .abort_all_fades()
    .await
    .map_err(|err| err.to_string())
```

- [ ] **Step 6: Run lifecycle command regression tests**

Run: `cargo nextest run -p advanced-show-control commands::tests::active_command_bus_tracks_current_bus connected_runtime_installs_scene_recall_fader_handle replacement_connect_cleanup_aborts_existing_runtime_and_clears_command_bus reconnect_timed_out_aborts_runtime_and_clears_command_bus_for_matching_attempt`

Expected: PASS.

- [ ] **Step 7: Run compiler check**

Run: `cargo check --workspace --all-targets`

Expected: PASS.

- [ ] **Step 8: Commit lifecycle managed state**

Run: `git status --short`

Then commit:

```bash
git add src-tauri/src/lifecycle/mod.rs src-tauri/src/commands.rs src-tauri/src/main.rs
git commit -m "refactor: manage app lifecycle state"
```

---

### Task 3: Move Runtime Handle Operations Behind `AppLifecycle`

**Files:**
- Modify: `src-tauri/src/lifecycle/mod.rs`
- Modify: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: `AppLifecycle` from Task 2 and existing `ShellState` runtime handle methods.
- Produces lifecycle methods:
  - `pub async fn clear_runtime_handles(&self, state: &ShellState, generation: u64)`
  - `pub async fn abort_current_runtime(&self, state: &ShellState)`
  - `pub async fn clear_runtime_handles_with_active_generation(&self, state: &ShellState, generation: u64)`
  - `pub async fn install_runtime_handles(&self, state: &ShellState, generation: u64, next: RuntimeHandles) -> Result<(), RuntimeHandles>`

- [ ] **Step 1: Write lifecycle delegation tests**

Add these tests to `src-tauri/src/lifecycle/mod.rs`:

```rust
#[tokio::test]
async fn lifecycle_installs_command_bus_with_runtime_handles() {
    let lifecycle = AppLifecycle::default();
    let state = crate::app_state::ShellState::default();
    let (generation, _) = state.disconnect().await;
    let command_bus = AppCommandBus::new(AppEventBus::default());

    lifecycle
        .install_runtime_handles(
            &state,
            generation,
            crate::app_state::RuntimeHandles {
                command_bus: Some(command_bus),
                ..Default::default()
            },
        )
        .await
        .expect("current generation install should succeed");

    assert!(lifecycle.current_command_bus().await.is_some());
}

#[tokio::test]
async fn lifecycle_clear_runtime_handles_clears_current_bus() {
    let lifecycle = AppLifecycle::default();
    let state = crate::app_state::ShellState::default();
    let (generation, _) = state.disconnect().await;
    let command_bus = AppCommandBus::new(AppEventBus::default());

    lifecycle
        .install_runtime_handles(
            &state,
            generation,
            crate::app_state::RuntimeHandles {
                command_bus: Some(command_bus),
                ..Default::default()
            },
        )
        .await
        .expect("current generation install should succeed");

    lifecycle.clear_runtime_handles(&state, generation).await;

    assert!(lifecycle.current_command_bus().await.is_none());
}
```

- [ ] **Step 2: Run the new tests and verify failure**

Run: `cargo nextest run -p advanced-show-control lifecycle::tests::lifecycle_`

Expected: FAIL because the lifecycle runtime handle methods do not exist yet.

- [ ] **Step 3: Implement lifecycle delegation methods**

Add these methods to `impl AppLifecycle` in `src-tauri/src/lifecycle/mod.rs`:

```rust
pub async fn clear_runtime_handles(&self, state: &crate::app_state::ShellState, generation: u64) {
    state
        .clear_runtime_handles(generation, &self.command_bus)
        .await;
}

pub async fn abort_current_runtime(&self, state: &crate::app_state::ShellState) {
    state.abort_current_runtime(&self.command_bus).await;
}

pub async fn clear_runtime_handles_with_active_generation(
    &self,
    state: &crate::app_state::ShellState,
    generation: u64,
) {
    state
        .clear_runtime_handles_with_active_generation(generation, &self.command_bus)
        .await;
}

pub async fn install_runtime_handles(
    &self,
    state: &crate::app_state::ShellState,
    generation: u64,
    next: crate::app_state::RuntimeHandles,
) -> Result<(), crate::app_state::RuntimeHandles> {
    state
        .install_runtime_handles(generation, next, &self.command_bus)
        .await
}
```

- [ ] **Step 4: Route command runtime operations through lifecycle**

In `src-tauri/src/commands.rs`, replace calls like:

```rust
state.clear_runtime_handles(generation, &active_command_bus).await;
state.abort_current_runtime(&active_command_bus).await;
state.clear_runtime_handles_with_active_generation(generation, &active_command_bus).await;
state.install_runtime_handles(generation, runtime_handles, active_command_bus).await;
```

with:

```rust
lifecycle.clear_runtime_handles(&state, generation).await;
lifecycle.abort_current_runtime(&state).await;
lifecycle.clear_runtime_handles_with_active_generation(&state, generation).await;
lifecycle.install_runtime_handles(state, generation, runtime_handles).await;
```

For private helpers that currently accept `active_command_bus: ActiveCommandBus`, add a `lifecycle: crate::lifecycle::AppLifecycle` parameter when the helper performs runtime handle operations. Keep `ActiveCommandBus` parameters only for `spawn_shell_state_projector` and `apply_projector_event` until the projector phase because those functions still need the current holder directly.

- [ ] **Step 5: Keep projector helper behavior unchanged**

When calling `spawn_shell_state_projector`, pass:

```rust
lifecycle.command_bus_holder()
```

Do not move `spawn_shell_state_projector`, `apply_projector_event`, or `emit_snapshot` in this task.

- [ ] **Step 6: Run lifecycle and runtime cleanup regression tests**

Run: `cargo nextest run -p advanced-show-control lifecycle::tests::lifecycle_ commands::tests::stale_runtime_install_does_not_emit_current_snapshot commands::tests::connected_runtime_installs_scene_recall_fader_handle commands::tests::reconnect_timed_out_aborts_runtime_and_clears_command_bus_for_matching_attempt commands::tests::stale_reconnect_timed_out_does_not_clear_newer_reconnect_state`

Expected: PASS.

- [ ] **Step 7: Commit runtime lifecycle delegation**

Run: `git status --short`

Then commit:

```bash
git add src-tauri/src/lifecycle/mod.rs src-tauri/src/commands.rs
git commit -m "refactor: route runtime handles through lifecycle"
```

---

### Task 4: Move Tauri Builder Setup Into `ui/`

**Files:**
- Modify: `src-tauri/src/ui/mod.rs`
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: existing `ShellState`, `AppLifecycle`, logging setup, and existing command functions.
- Produces:
  - `pub fn build_app<R: tauri::Runtime>() -> tauri::Builder<R>`
  - `main.rs` that calls `advanced_show_control::ui::build_app().run(...)`

- [ ] **Step 1: Add a UI module test for managed state setup**

Add this test module to `src-tauri/src/ui/mod.rs` below the docs:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn build_app_constructs_builder() {
        let _builder = super::build_app::<tauri::Wry>();
    }
}
```

- [ ] **Step 2: Run the UI test and verify failure**

Run: `cargo nextest run -p advanced-show-control ui::tests::build_app_constructs_builder`

Expected: FAIL because `build_app` does not exist yet.

- [ ] **Step 3: Implement `ui::build_app` with existing setup**

Replace `src-tauri/src/ui/mod.rs` contents with:

```rust
//! Tauri adapter layer.
//!
//! This module contains command registration and frontend serialization
//! boundaries. Business logic should route through `crate::runtime::commands::AppCommandBus`.

use crate::app_state::ShellState;
use crate::commands;
use crate::lifecycle::AppLifecycle;
use crate::logging;
use tauri::Manager;

pub fn build_app<R: tauri::Runtime>() -> tauri::Builder<R> {
    tauri::Builder::default()
        .manage(ShellState::default())
        .manage(AppLifecycle::default())
        .setup(|app| {
            let shell_state = (*app.state::<ShellState>()).clone();
            let logging_guard = logging::init_logging(app.handle(), shell_state.clone())?;
            app.manage(logging_guard);
            tracing::info!(event = "app_started", "Starting Advanced Show Control");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_status,
            commands::refresh_lv1_discovery,
            commands::new_show_file,
            commands::open_show_file_dialog,
            commands::save_show_file,
            commands::save_show_file_as_dialog,
            commands::set_scene_duration_ms,
            commands::select_scene_config,
            commands::cue_scene,
            commands::recall_scene,
            commands::connect_lv1,
            commands::connect_lv1_system,
            commands::attempt_reconnect_lv1,
            commands::startup_auto_connect_lv1,
            commands::disconnect_lv1,
            commands::reconnect_timed_out,
            commands::abort_all_fades,
            commands::store_scene_config,
            commands::set_channel_scoped,
            commands::set_all_channels_scoped,
            commands::set_scene_scope_faders_enabled,
            commands::set_scene_scope_pan_enabled,
            commands::set_lockout,
        ])
}

#[cfg(test)]
mod tests {
    #[test]
    fn build_app_constructs_builder() {
        let _builder = super::build_app::<tauri::Wry>();
    }
}
```

- [ ] **Step 4: Make `main.rs` a thin entry point**

Replace `src-tauri/src/main.rs` with:

```rust
fn main() {
    advanced_show_control::ui::build_app()
        .run(tauri::generate_context!())
        .expect("failed to run Advanced Show Control");
}
```

- [ ] **Step 5: Run UI and command exposure tests**

Run: `cargo nextest run -p advanced-show-control ui::tests::build_app_constructs_builder commands::tests::connection_chooser_commands_are_exposed commands::tests::scene_store_commands_are_exposed`

Expected: PASS.

- [ ] **Step 6: Run Tauri binary build check**

Run: `cargo build -p advanced-show-control --bin advanced-show-control`

Expected: PASS.

- [ ] **Step 7: Commit UI builder move**

Run: `git status --short`

Then commit:

```bash
git add src-tauri/src/ui/mod.rs src-tauri/src/main.rs
git commit -m "refactor: move tauri setup into ui module"
```

---

### Task 5: Move Tauri Command Adapter Functions Into `ui::commands`

**Files:**
- Create: `src-tauri/src/ui/commands.rs`
- Modify: `src-tauri/src/ui/mod.rs`
- Modify: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: existing public Tauri command functions from `crate::commands`.
- Produces: same Tauri command names under `crate::ui::commands`.
- Keeps business/runtime helper code in `crate::commands` for now unless a helper must remain private to the command adapter.

- [ ] **Step 1: Create `ui::commands` re-export module first**

Create `src-tauri/src/ui/commands.rs` with:

```rust
//! Tauri command adapter exports.
//!
//! This module is the frontend command registration surface. During this phase
//! it re-exports the existing command implementations so behavior and command
//! names stay unchanged while the adapter boundary moves into `ui/`.

pub use crate::commands::{
    abort_all_fades, attempt_reconnect_lv1, connect_lv1, connect_lv1_system, cue_scene,
    disconnect_lv1, get_app_status, new_show_file, open_show_file_dialog, recall_scene,
    reconnect_timed_out, refresh_lv1_discovery, save_show_file, save_show_file_as_dialog,
    select_scene_config, set_all_channels_scoped, set_channel_scoped, set_lockout,
    set_scene_duration_ms, set_scene_scope_faders_enabled, set_scene_scope_pan_enabled,
    startup_auto_connect_lv1, store_scene_config,
};
```

- [ ] **Step 2: Export `ui::commands`**

In `src-tauri/src/ui/mod.rs`, add near the top:

```rust
pub mod commands;
```

Then change every `commands::name` in `generate_handler!` to continue resolving to `ui::commands::name`. Because `ui/mod.rs` imports `crate::commands` today, rename that import:

```rust
use crate::commands as command_impl;
```

Then use the local module in the handler list:

```rust
commands::get_app_status,
commands::refresh_lv1_discovery,
commands::new_show_file,
commands::open_show_file_dialog,
commands::save_show_file,
commands::save_show_file_as_dialog,
commands::set_scene_duration_ms,
commands::select_scene_config,
commands::cue_scene,
commands::recall_scene,
commands::connect_lv1,
commands::connect_lv1_system,
commands::attempt_reconnect_lv1,
commands::startup_auto_connect_lv1,
commands::disconnect_lv1,
commands::reconnect_timed_out,
commands::abort_all_fades,
commands::store_scene_config,
commands::set_channel_scoped,
commands::set_all_channels_scoped,
commands::set_scene_scope_faders_enabled,
commands::set_scene_scope_pan_enabled,
commands::set_lockout,
```

If the renamed `command_impl` import is unused after this edit, remove it.

- [ ] **Step 3: Add adapter boundary test**

Add this test to `src-tauri/src/ui/mod.rs` tests:

```rust
#[test]
fn command_adapter_exports_existing_command_names() {
    let _ = super::commands::get_app_status;
    let _ = super::commands::connect_lv1;
    let _ = super::commands::disconnect_lv1;
    let _ = super::commands::recall_scene;
    let _ = super::commands::set_lockout;
}
```

- [ ] **Step 4: Run adapter tests**

Run: `cargo nextest run -p advanced-show-control ui::tests::command_adapter_exports_existing_command_names ui::tests::build_app_constructs_builder`

Expected: PASS.

- [ ] **Step 5: Run Tauri command exposure regressions**

Run: `cargo nextest run -p advanced-show-control commands::tests::connection_chooser_commands_are_exposed commands::tests::scene_store_commands_are_exposed`

Expected: PASS.

- [ ] **Step 6: Commit adapter command boundary**

Run: `git status --short`

Then commit:

```bash
git add src-tauri/src/ui/mod.rs src-tauri/src/ui/commands.rs
git commit -m "refactor: expose tauri commands through ui module"
```

---

### Task 6: Update Architecture Docs For Lifecycle/UI Boundary

**Files:**
- Modify: `docs/architecture.md`
- Modify: `AGENTS.md`

**Interfaces:**
- Consumes: lifecycle and UI boundary introduced in Tasks 1-5.
- Produces: docs that describe current intermediate state accurately.

- [ ] **Step 1: Update architecture overview wording**

In `docs/architecture.md`, replace the overview boundary list item:

```markdown
- `Tauri Shell`
```

with:

```markdown
- `Tauri UI Adapter`
- `AppLifecycle`
```

Then replace:

```markdown
- `Tauri Shell` owns UI projection, shell commands, and user-facing state derived from the runtime.
```

with:

```markdown
- `Tauri UI Adapter` owns Tauri setup, command registration, dialogs, and frontend serialization boundaries.
- `AppLifecycle` owns the current runtime command-bus holder seam and delegates generation-sensitive runtime handle installation/cleanup to `ShellState` until the projector and command-boundary phases replace that temporary split.
```

- [ ] **Step 2: Update file structure docs**

In `docs/architecture.md`, update the Tauri shell file bullets to:

```markdown
- `src-tauri/src/ui/` for Tauri setup and frontend command adapter exports.
- `src-tauri/src/lifecycle/` for app runtime lifecycle ownership seams.
- `src-tauri/src/commands.rs` for existing command implementations during the transition.
- `src-tauri/src/app_state/` for `ShellState`, projections, logs, show-file mapping, and view models until later projector/show phases remove that temporary ownership.
- `src-tauri/src/connection_state.rs` and `src-tauri/src/connection_preferences.rs` for shell-facing connection state.
```

- [ ] **Step 3: Update AGENTS project context if needed**

In `AGENTS.md`, keep the current single-crate layout. If the architecture bullets still describe `ShellState` as the Tauri-side projection and command surface, update only that bullet to:

```markdown
- `ShellState` is the current Tauri-side projection state holder during the transition.
- `AppLifecycle` is the Tauri-side runtime lifecycle seam and current command-bus holder.
```

Do not claim `ShellState` or `ActiveCommandBus` has been eliminated.

- [ ] **Step 4: Run docs-adjacent verification**

Run: `cargo check --workspace --all-targets`

Expected: PASS.

- [ ] **Step 5: Commit docs update**

Run: `git status --short`

Then commit:

```bash
git add docs/architecture.md AGENTS.md
git commit -m "docs: describe lifecycle and ui boundary"
```

---

### Task 7: Full Phase Verification

**Files:**
- No source edits unless verification exposes failures.

**Interfaces:**
- Consumes: completed lifecycle and UI boundary phase.
- Produces: verified baseline for later `ShowEvent`, command-routing, and projector phases.

- [ ] **Step 1: Run Rust formatting**

Run: `cargo fmt --all -- --check`

Expected: PASS.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 3: Run Rust tests**

Run: `cargo nextest run --workspace`

Expected: PASS.

- [ ] **Step 4: Build workspace**

Run: `cargo build --workspace`

Expected: PASS.

- [ ] **Step 5: Build preserved CLI binary**

Run: `cargo build -p advanced-show-control --bin lv1-probe`

Expected: PASS.

- [ ] **Step 6: Verify Tauri app binary remains default runnable target**

Run: `cargo nextest run -p advanced-show-control tauri_dev_uses_app_binary_by_default`

Expected: PASS.

- [ ] **Step 7: Run frontend typecheck**

Run: `npm --prefix ui run typecheck`

Expected: PASS.

- [ ] **Step 8: Run non-interactive Tauri build**

Run: `npm run tauri -- build`

Expected: PASS. A warning that the bundle identifier `com.advancedshowcontrol.app` ends with `.app` is already known and non-fatal.

- [ ] **Step 9: Do not commit verification-only output**

Run: `git status --short`

Expected: no source changes. If a verification command changed build artifacts only, do not commit them.

---

## Self-Review Notes

- This plan implements only spec phases 5-6.
- This plan intentionally preserves `ShellState`, direct `app-status-changed` emits, command-returned `AppViewState`, and the old shell-state projector.
- This plan moves ownership seams without changing frontend behavior.
- `ActiveCommandBus` remains as a temporary type name but moves under `lifecycle/`; full removal remains phase 17.
- The deeper command boundary work begins in the next plan with `AppEvent::Show(ShowEvent)` and low-risk show/app command routing.

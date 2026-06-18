# Single Crate Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the Rust codebase to a single Tauri-rooted crate while preserving current behavior and adding the first scaffolding for the new command/projection architecture.

**Architecture:** Keep `src-tauri/` as the Tauri project root, move current root core modules into `src-tauri/src/`, preserve the CLI as `src-tauri/src/bin/lv1-probe.rs`, and keep root `Cargo.toml` as a workspace-only manifest. This plan does not remove `ShellState`, direct emits, command-return snapshots, or `ActiveCommandBus`; it prepares the tree so later plans can do that safely.

**Tech Stack:** Rust 2024, Tokio, Tauri 2, cargo-nextest, React/TypeScript frontend, existing workspace CI commands.

## Global Constraints

- Final Rust package name is `advanced-show-control`.
- Tauri project root remains `src-tauri/`.
- CLI/probe binary remains supported as `lv1-probe`.
- Keep `tauri.conf.json`, `build.rs`, capabilities, icons, and generated schemas in `src-tauri/`.
- Each task must leave the app in working order.
- Do not change LV1 protocol behavior.
- Do not weaken lockout, exact scene identity, generation guards, disconnect behavior, manual override, abort, or overlap safety behavior.
- Do not route logs through `AppEventBus`.
- Do not remove `ShellState`, `ActiveCommandBus`, direct emits, or command-return snapshots in this plan.

---

## File Structure

Files created or moved by this plan:

- Move: `src/lib.rs` -> `src-tauri/src/lib.rs`
- Move: `src/fade/` -> `src-tauri/src/fade/`
- Move: `src/lv1/` -> `src-tauri/src/lv1/`
- Move: `src/osc.rs` -> `src-tauri/src/osc.rs`
- Move: `src/runtime/` -> `src-tauri/src/runtime/`
- Move: `src/scene_recall/` -> `src-tauri/src/scene_recall/`
- Move: `src/show/` -> `src-tauri/src/show/`
- Move: `src/vegas.rs` -> `src-tauri/src/vegas.rs`
- Move: `src/main.rs` -> `src-tauri/src/bin/lv1-probe.rs`
- Modify: `Cargo.toml` to become workspace-only.
- Modify: `src-tauri/Cargo.toml` to become the single package named `advanced-show-control`.
- Modify: `src-tauri/src/main.rs` to include core modules through the unified crate.
- Modify: `tests/*.rs` integration tests so they live under the remaining package.
- Create: `src-tauri/src/ui/mod.rs` as Tauri adapter module placeholder.
- Create: `src-tauri/src/projector/mod.rs` as projector module placeholder.
- Create: `src-tauri/src/lifecycle/mod.rs` as app lifecycle owner placeholder.
- Create: `src-tauri/src/show/events.rs` as future `ShowEvent` home.
- Create: `src-tauri/src/show/commands.rs` as future show command handler home.
- Modify: docs and frontend type comments that point at the old two-crate layout.

---

### Task 1: Characterize Current Emission And CLI Behavior

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/logging.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: existing `emit_snapshot`, `ui_log_projector`, and CLI parser tests.
- Produces: characterization tests proving current direct emit sites, log direct emit behavior, 10 Hz projector behavior, and CLI parser availability.

- [ ] **Step 1: Locate existing tests before adding coverage**

Run: `cargo nextest run --workspace projector_ ui_layer_projects parses_pan_family_smoke_test_command`

Expected: existing matching tests pass or nextest reports only unmatched filter names for tests that do not exist yet.

- [ ] **Step 2: Add a test that documents command helper direct emit behavior**

In `src-tauri/src/commands.rs`, inside the existing `#[cfg(test)] mod tests`, add this test near other `app-status-changed` listener tests:

```rust
#[tokio::test]
async fn emit_snapshot_directly_emits_app_status_changed() {
    let app = mock_app();
    let handle = app.handle().clone();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_for_listener = observed.clone();

    handle.listen_any("app-status-changed", move |event| {
        let payload: serde_json::Value = serde_json::from_str(event.payload())
            .expect("app-status-changed payload should be valid JSON");
        observed_for_listener.lock().unwrap().push(payload);
    });

    let state = ShellState::default();
    let snapshot = state.snapshot().await;
    emit_snapshot(&handle, &snapshot);
    tokio::task::yield_now().await;

    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0]["stateVersion"], snapshot.state_version);
}
```

- [ ] **Step 3: Run the command emit characterization test**

Run: `cargo nextest run -p advanced-show-control-tauri emit_snapshot_directly_emits_app_status_changed`

Expected: PASS.

- [ ] **Step 4: Add a test that documents CLI binary parsing remains required**

In `src/main.rs`, inside the existing CLI tests module, add this parser test if an equivalent test does not already exist:

```rust
#[test]
fn parses_discover_command_for_probe_binary() {
    let cli = parse_cli_from([
        "lv1-probe",
        "discover",
        "--timeout-ms",
        "100",
        "--json",
    ])
    .expect("discover command should parse");

    match cli.command {
        Command::Discover {
            timeout_ms,
            filter_host,
            json,
        } => {
            assert_eq!(timeout_ms, 100);
            assert_eq!(filter_host, None);
            assert!(json);
        }
        other => panic!("expected discover command, got {other:?}"),
    }
}
```

- [ ] **Step 5: Run the CLI parser characterization test**

Run: `cargo nextest run -p advanced-show-control --bin advanced-show-control parses_discover_command_for_probe_binary`

Expected: PASS.

- [ ] **Step 6: Run related characterization tests**

Run: `cargo nextest run -p advanced-show-control-tauri projector_ ui_layer_projects`

Expected: PASS.

- [ ] **Step 7: Commit characterization coverage**

Run: `git status --short`

Then stage only the changed test files and commit:

```bash
git add src-tauri/src/commands.rs src/main.rs
git commit -m "test: characterize current projection boundaries"
```

---

### Task 2: Convert Root Manifest To Workspace-Only And Merge Dependencies

**Files:**
- Modify: `Cargo.toml`
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: current root package dependencies and Tauri package dependencies.
- Produces: one package named `advanced-show-control` at `src-tauri/`, with root as workspace-only.

- [ ] **Step 1: Update root `Cargo.toml`**

Replace root `Cargo.toml` with:

```toml
[workspace]
members = ["src-tauri"]
resolver = "2"
```

- [ ] **Step 2: Update `src-tauri/Cargo.toml` package name and dependencies**

Replace `src-tauri/Cargo.toml` with:

```toml
[package]
name = "advanced-show-control"
version = "0.1.0"
description = "Desktop app for Advanced Show Control"
edition = "2024"
license = "GPL-3.0-or-later"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
clap = { version = "4.5", features = ["derive"] }
dirs = "6"
rfd = "0.15"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
socket2 = { version = "0.5", features = ["all"] }
tauri = { version = "2", features = [] }
thiserror = "2.0"
tokio = { version = "1", features = ["full", "test-util"] }
tracing = { version = "0.1.44", features = ["attributes"] }
tracing-appender = "0.2.5"
tracing-subscriber = { version = "0.3.23", features = ["env-filter", "fmt", "json"] }
uuid = { version = "1.8", features = ["v4"] }

[dev-dependencies]
tauri = { version = "2", features = ["test"] }
```

- [ ] **Step 3: Run metadata check**

Run: `cargo metadata --no-deps`

Expected: one workspace package named `advanced-show-control` rooted at `src-tauri/Cargo.toml`.

- [ ] **Step 4: Commit manifest conversion**

Run: `git status --short`

Then commit:

```bash
git add Cargo.toml src-tauri/Cargo.toml
git commit -m "build: make tauri crate the rust package"
```

---

### Task 3: Move Core Modules Into The Tauri Crate

**Files:**
- Move: `src/fade/` -> `src-tauri/src/fade/`
- Move: `src/lv1/` -> `src-tauri/src/lv1/`
- Move: `src/runtime/` -> `src-tauri/src/runtime/`
- Move: `src/scene_recall/` -> `src-tauri/src/scene_recall/`
- Move: `src/show/` -> `src-tauri/src/show/`
- Move: `src/osc.rs` -> `src-tauri/src/osc.rs`
- Move: `src/vegas.rs` -> `src-tauri/src/vegas.rs`
- Move: `src/lib.rs` -> `src-tauri/src/lib.rs`
- Move: `src/main.rs` -> `src-tauri/src/bin/lv1-probe.rs`
- Move: `tests/*.rs` -> `src-tauri/tests/*.rs`

**Interfaces:**
- Consumes: root core module tree.
- Produces: unified crate module tree with preserved library and CLI binary targets.

- [ ] **Step 1: Create target directories**

Run: `mkdir -p src-tauri/src/bin src-tauri/tests`

Expected: directories exist.

- [ ] **Step 2: Move files with git**

Run these commands:

```bash
git mv src/fade src-tauri/src/fade
git mv src/lv1 src-tauri/src/lv1
git mv src/runtime src-tauri/src/runtime
git mv src/scene_recall src-tauri/src/scene_recall
git mv src/show src-tauri/src/show
git mv src/osc.rs src-tauri/src/osc.rs
git mv src/vegas.rs src-tauri/src/vegas.rs
git mv src/lib.rs src-tauri/src/lib.rs
git mv src/main.rs src-tauri/src/bin/lv1-probe.rs
git mv tests/fade_engine.rs src-tauri/tests/fade_engine.rs
git mv tests/runtime_bus.rs src-tauri/tests/runtime_bus.rs
git mv tests/lv1_actor.rs src-tauri/tests/lv1_actor.rs
```

- [ ] **Step 3: Remove empty root `src/` and `tests/` directories if empty**

Run: `rmdir src tests`

Expected: succeeds if directories are empty; if either directory does not exist or is already removed, continue.

- [ ] **Step 4: Run formatting check to expose moved-file syntax issues**

Run: `cargo fmt --all -- --check`

Expected: may fail due unresolved imports in later task, but should not fail due malformed files. If it formats cleanly, continue.

- [ ] **Step 5: Commit physical move only**

Run: `git status --short`

Then commit:

```bash
git add -A src src-tauri tests
git commit -m "refactor: move core modules into tauri crate"
```

---

### Task 4: Fix Unified-Crate Imports And Module Declarations

**Files:**
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/bin/lv1-probe.rs`
- Modify: `src-tauri/src/**/*.rs`
- Modify: `src-tauri/tests/*.rs`

**Interfaces:**
- Consumes: moved module tree.
- Produces: compiling single crate with `crate::...` and `advanced_show_control::...` references only where integration tests or binary targets require crate-name imports.

- [ ] **Step 1: Add library module declarations for Tauri-side modules**

In `src-tauri/src/lib.rs`, keep the moved core exports and add exports for Tauri modules that tests or binaries need:

```rust
pub mod app_state;
pub mod commands;
pub mod connection_preferences;
pub mod connection_state;
pub mod diagnostics;
pub mod fade;
pub mod logging;
pub mod lv1;
pub mod osc;
pub mod runtime;
pub mod scene_recall;
pub mod show;
pub mod show_file;
pub mod time;
pub mod vegas;
```

- [ ] **Step 2: Update `src-tauri/src/main.rs` to use library modules**

Replace the top module declarations in `src-tauri/src/main.rs`:

```rust
mod app_state;
mod commands;
mod connection_preferences;
mod connection_state;
mod diagnostics;
mod logging;
mod show_file;
mod time;

use app_state::ShellState;
use commands::ActiveCommandBus;
```

with:

```rust
use advanced_show_control::app_state::ShellState;
use advanced_show_control::commands;
use advanced_show_control::commands::ActiveCommandBus;
use advanced_show_control::logging;
```

- [ ] **Step 3: Update CLI binary imports**

In `src-tauri/src/bin/lv1-probe.rs`, replace imports beginning with `advanced_show_control::` only if needed after the package rename. Keep binary-to-library imports as `advanced_show_control::...` because the package name `advanced-show-control` exposes library crate `advanced_show_control` by default.

- [ ] **Step 4: Update Tauri library imports from old external crate to crate-local imports**

In files under `src-tauri/src/` that are part of the library and currently import `advanced_show_control::...`, replace them with `crate::...`.

Examples:

```rust
use advanced_show_control::runtime::commands::AppCommandBus;
```

becomes:

```rust
use crate::runtime::commands::AppCommandBus;
```

And fully qualified paths such as:

```rust
advanced_show_control::lv1::types::ConnectionStatus::Connected
```

become:

```rust
crate::lv1::types::ConnectionStatus::Connected
```

- [ ] **Step 5: Keep integration tests using crate-name imports**

Files under `src-tauri/tests/` should import the library as `advanced_show_control::...` because integration tests are external to the library target.

- [ ] **Step 6: Run compiler check**

Run: `cargo check --workspace --all-targets`

Expected: PASS.

- [ ] **Step 7: Run targeted tests**

Run: `cargo nextest run --workspace emit_snapshot_directly_emits_app_status_changed parses_discover_command_for_probe_binary`

Expected: PASS.

- [ ] **Step 8: Commit import fixes**

Run: `git status --short`

Then commit:

```bash
git add -A src-tauri
git commit -m "refactor: fix imports after crate merge"
```

---

### Task 5: Update Documentation And Frontend Type References For Single Crate

**Files:**
- Modify: `AGENTS.md`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`
- Modify: `ui/src/types.ts`

**Interfaces:**
- Consumes: unified crate layout.
- Produces: docs and comments that no longer describe two Rust crates.

- [ ] **Step 1: Update `AGENTS.md` project layout**

Replace the old Rust layout bullets with:

```markdown
- `src-tauri/` contains the single Rust/Tauri crate, `advanced-show-control`. Core Rust modules such as `lv1/`, `fade/`, `scene_recall/`, `show/`, and `runtime/` live under `src-tauri/src/` alongside Tauri adapter modules.
- `src-tauri/src/bin/lv1-probe.rs` contains the preserved LV1 probe/developer CLI binary.
- `ui/` contains the React/TypeScript frontend.
```

- [ ] **Step 2: Update verification command examples in `AGENTS.md`**

Replace package-specific examples that use `advanced-show-control-tauri` with `advanced-show-control`. Keep command names otherwise unchanged.

- [ ] **Step 3: Update `docs/architecture.md` file structure section**

Replace the two-crate Rust layout with a single-crate layout:

```markdown
Rust modules live under `src-tauri/src/` in the `advanced-show-control` package. Tauri-specific adapter code and core app modules are separated by module boundaries, not crate boundaries.
```

- [ ] **Step 4: Update `docs/roadmap.md` CLI/core/Tauri language**

Replace references to separate core/Tauri/CLI crates with wording that describes one Rust crate and a preserved CLI binary.

- [ ] **Step 5: Update frontend type sync comment**

In `ui/src/types.ts`, replace line 1 with:

```ts
// Keep these types in sync with src-tauri/src/app_state/view.rs; Rust owns AppViewState snapshots; TS mirrors serialized Tauri event payloads; update both and run npm run typecheck.
```

- [ ] **Step 6: Verify docs and frontend typecheck**

Run: `npm --prefix ui run typecheck`

Expected: PASS.

- [ ] **Step 7: Commit docs updates**

Run: `git status --short`

Then commit:

```bash
git add AGENTS.md docs/architecture.md docs/roadmap.md ui/src/types.ts
git commit -m "docs: update rust layout references"
```

---

### Task 6: Add Target Module Skeletons Without Behavior Changes

**Files:**
- Create: `src-tauri/src/ui/mod.rs`
- Create: `src-tauri/src/projector/mod.rs`
- Create: `src-tauri/src/lifecycle/mod.rs`
- Create: `src-tauri/src/show/events.rs`
- Create: `src-tauri/src/show/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/show/mod.rs`

**Interfaces:**
- Consumes: unified crate module tree.
- Produces: named module homes for later plans without changing behavior.

- [ ] **Step 1: Create `src-tauri/src/ui/mod.rs`**

```rust
//! Tauri adapter layer.
//!
//! This module will contain command registration and frontend serialization
//! boundaries. Business logic should route through `crate::runtime::commands::AppCommandBus`.
```

- [ ] **Step 2: Create `src-tauri/src/projector/mod.rs`**

```rust
//! AppViewState projection and `app-status-changed` emission.
//!
//! The projector will become the only backend owner of app-status-changed emission.
```

- [ ] **Step 3: Create `src-tauri/src/lifecycle/mod.rs`**

```rust
//! App runtime lifecycle ownership.
//!
//! This module will replace the temporary ActiveCommandBus holder and own
//! runtime task handles, generation, command bus installation, and projector startup.
```

- [ ] **Step 4: Create `src-tauri/src/show/events.rs`**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShowEvent {
    SnapshotChanged,
}
```

- [ ] **Step 5: Create `src-tauri/src/show/commands.rs`**

```rust
//! Show-owned application command handlers.
//!
//! Later tasks will move show/session command validation and mutation here.
```

- [ ] **Step 6: Export new modules**

In `src-tauri/src/lib.rs`, add:

```rust
pub mod lifecycle;
pub mod projector;
pub mod ui;
```

In `src-tauri/src/show/mod.rs`, add:

```rust
pub mod commands;
pub mod events;
```

- [ ] **Step 7: Run module compile check**

Run: `cargo check --workspace --all-targets`

Expected: PASS.

- [ ] **Step 8: Commit module skeletons**

Run: `git status --short`

Then commit:

```bash
git add src-tauri/src/lib.rs src-tauri/src/ui/mod.rs src-tauri/src/projector/mod.rs src-tauri/src/lifecycle/mod.rs src-tauri/src/show/mod.rs src-tauri/src/show/events.rs src-tauri/src/show/commands.rs
git commit -m "refactor: add target architecture modules"
```

---

### Task 7: Run Full Foundation Verification

**Files:**
- No source edits unless verification exposes failures.

**Interfaces:**
- Consumes: completed single-crate foundation.
- Produces: verified baseline for later command/projection refactor plans.

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

- [ ] **Step 5: Build CLI binary explicitly**

Run: `cargo build -p advanced-show-control --bin lv1-probe`

Expected: PASS.

- [ ] **Step 6: Run frontend typecheck**

Run: `npm --prefix ui run typecheck`

Expected: PASS.

- [ ] **Step 7: Research and run non-interactive Tauri build verification**

Run: `npm run tauri -- build`

Expected: PASS, or a documented missing-platform-dependency failure. If this command is wrong for this repository, record the correct command in `docs/superpowers/specs/2026-06-19-single-crate-command-projection-architecture-design.md` and rerun it.

- [ ] **Step 8: Commit verification-only doc update if needed**

If Step 7 required a spec update, commit it:

```bash
git add docs/superpowers/specs/2026-06-19-single-crate-command-projection-architecture-design.md
git commit -m "docs: record tauri verification command"
```

If no file changed, do not commit.

---

## Self-Review Notes

- This plan covers the spec through the single-crate foundation and target-module scaffolding only.
- It intentionally does not implement `ShowEvent` routing, app lifecycle owner behavior, projector cache replacement, logging input migration, React command contract changes, `ShellState` removal, or `ActiveCommandBus` removal.
- Those changes require follow-up plans so every step can leave the app working.

# Organization Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve maintainability by splitting oversized Rust and React files into focused modules without changing behavior.

**Architecture:** This is an extraction-only refactor. Public command names, Tauri event names, serialized DTO fields, tests, and runtime behavior stay the same. Each task moves one responsibility boundary at a time, then runs targeted and full verification before continuing.

**Tech Stack:** Rust 2024, Tauri 2, Tokio, React, TypeScript, Vite, Tailwind.

---

## File Structure

### Rust Core: `src/lv1/`

- Modify: `src/lv1/mod.rs`
  - Export the new focused LV1 modules.
- Create: `src/lv1/model.rs`
  - Own `ConnectionStatus`, `SceneState`, `SceneListEntry`, `ChannelInfo`, and `Lv1StateSnapshot`.
- Create: `src/lv1/messages.rs`
  - Own `Lv1ActorError`, `Lv1Command`, and `Lv1Event`.
- Create: `src/lv1/parsers.rs`
  - Own parser helpers currently embedded in `state.rs`, including channel and scene-list parsers.
- Modify: `src/lv1/state.rs`
  - Keep actor handle, spawn function, runtime state, and event loop only.

### Fade Core: `src/fade/`

- Modify: `src/fade/mod.rs`
  - Export extracted engine modules if needed.
- Create: `src/fade/types.rs`
  - Own public fade DTOs and events: `FadeTarget`, `FadeConfig`, `FadeCommand`, `FadeEvent`.
- Create: `src/fade/tick.rs`
  - Own `ActiveChannel`, tick constants, interpolation step decisions, and pure tick tests.
- Modify: `src/fade/engine.rs`
  - Keep actor handle, `spawn_engine`, runtime loop, subscribers, and LV1 command interaction.

### Tauri Shell: `src-tauri/src/app_state/`

- Delete-as-module-file after split: `src-tauri/src/app_state.rs`
- Create: `src-tauri/src/app_state/mod.rs`
  - Re-export public shell API used by `commands.rs`.
- Create: `src-tauri/src/app_state/view.rs`
  - Own `SceneSummary`, `ChannelSummary`, `FadeTarget`, `SceneFadeConfig`, `AppLogEntry`, `LogSource`, `LogSeverity`, `AppConnectionState`, `AppFadeState`, and `AppViewState`.
- Create: `src-tauri/src/app_state/shell.rs`
  - Own `RuntimeHandles`, `ShellState`, `ShellInner`, defaults, and snapshot entry points.
- Create: `src-tauri/src/app_state/capture.rs`
  - Own scene selection, listen mode, fade target enable/remove, scene duration, and LV1 fader capture mutation logic.
- Create: `src-tauri/src/app_state/events.rs`
  - Own LV1 and fade event application plus log creation.
- Create: `src-tauri/src/app_state/show_file_mapping.rs`
  - Own conversion between shell state and `ShowFile` DTOs.
- Modify: `src-tauri/src/commands.rs`
  - Update imports only; command behavior stays unchanged.

### React UI: `ui/src/`

- Modify: `ui/src/App.tsx`
  - Keep app-level state, Tauri event subscription, tab selection, and page composition.
- Create: `ui/src/commands.ts`
  - Own typed wrappers around `invoke` and refresh/error handling helpers.
- Create: `ui/src/components/Header.tsx`
  - Own title, current scene summary, show-file controls, status badges, lockout, finish, and abort controls.
- Create: `ui/src/components/ConnectionTab.tsx`
  - Own host/port connection form and connection status content.
- Create: `ui/src/components/SceneTab.tsx`
  - Own scene list, selected scene editor, listen mode controls, and fade target table composition.
- Create: `ui/src/components/LogsTab.tsx`
  - Own log rendering.
- Create: `ui/src/components/DurationInput.tsx`
  - Own duration draft/commit behavior.
- Create: `ui/src/components/StatusBadge.tsx`
  - Own shared status pill rendering.
- Create: `ui/src/components/ShowFileControls.tsx`
  - Own show-file button group.
- Create: `ui/src/format.ts`
  - Own `formatDurationSeconds`, dB formatting, and timestamp formatting helpers.

---

## Task 1: Split LV1 State Model From Actor Runtime

**Files:**
- Create: `src/lv1/model.rs`
- Create: `src/lv1/messages.rs`
- Create: `src/lv1/parsers.rs`
- Modify: `src/lv1/state.rs`
- Modify: `src/lv1/mod.rs`

- [ ] **Step 1: Snapshot current LV1 tests**

Run:

```bash
cargo test lv1::state --lib
```

Expected: all current `lv1::state` tests pass before moving code.

- [ ] **Step 2: Create `src/lv1/model.rs`**

Move these exact types out of `src/lv1/state.rs` without changing field names, derives, or visibility:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Disconnected,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneState {
    pub index: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneListEntry {
    pub index: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChannelInfo {
    pub group: i32,
    pub channel: i32,
    pub name: String,
    pub gain_db: f64,
    pub muted: bool,
}

#[derive(Debug, Clone)]
pub struct Lv1StateSnapshot {
    pub connection: ConnectionStatus,
    pub scene: Option<SceneState>,
    pub scene_list: Vec<SceneListEntry>,
    pub channels: Vec<ChannelInfo>,
}
```

- [ ] **Step 3: Create `src/lv1/messages.rs`**

Move `Lv1ActorError`, `Lv1Command`, and `Lv1Event` out of `src/lv1/state.rs`. Keep the existing variants unchanged. Add imports from `tokio::sync::{mpsc, oneshot}` and `crate::lv1::types::{Lv1StateSnapshot, SceneState, SceneListEntry, ChannelInfo}`.

- [ ] **Step 4: Create `src/lv1/parsers.rs`**

Move `CHANNELS_RECORD_STRIDE`, `parse_channels_batch`, and `parse_scene_list` out of `src/lv1/state.rs`. Keep parser behavior unchanged. Import `crate::osc::OscArg` and `crate::lv1::types::{ChannelInfo, SceneListEntry}`.

- [ ] **Step 5: Update `src/lv1/mod.rs`**

Add module exports:

```rust
pub mod discovery;
pub mod messages;
pub mod model;
pub mod parsers;
pub mod probe;
pub mod state;
pub mod tcp;
```

Preserve any existing module lines not shown here.

- [ ] **Step 6: Update imports in `src/lv1/state.rs`**

Replace local type/parser definitions with imports:

```rust
use crate::lv1::messages::{Lv1ActorError, Lv1Command, Lv1Event};
use crate::lv1::types::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry, SceneState};
use crate::lv1::parsers::{parse_channels_batch, parse_scene_list};
```

- [ ] **Step 7: Fix downstream imports**

Where files import model types from `lv1::state`, change those imports to `lv1::types`. Where files import `Lv1Event` or actor errors, change those imports to `lv1::events` or `lv1::messages`. Keep `spawn_actor` and `Lv1ActorHandle` imported from `lv1::actor` and `lv1::handle`.

Expected likely files:

```text
src-tauri/src/app_state.rs
src-tauri/src/show_file.rs
src-tauri/src/commands.rs
src/fade/engine.rs
```

- [ ] **Step 8: Verify LV1 split**

Run:

```bash
cargo test lv1 --lib
```

Expected: all LV1 tests pass.

---

## Task 2: Split Fade Engine Types And Pure Tick Logic

**Files:**
- Create: `src/fade/types.rs`
- Create: `src/fade/tick.rs`
- Modify: `src/fade/engine.rs`
- Modify: `src/fade/mod.rs`

- [ ] **Step 1: Snapshot current fade tests**

Run:

```bash
cargo test fade --lib
```

Expected: all current fade tests pass before moving code.

- [ ] **Step 2: Create `src/fade/types.rs`**

Move these public definitions from `src/fade/engine.rs` without changing names or variants:

```rust
use tokio::sync::mpsc;

use crate::fade::curve::FadeCurve;

#[derive(Debug, Clone)]
pub struct FadeTarget {
    pub group: i32,
    pub channel: i32,
    pub target_db: f64,
}

#[derive(Debug, Clone)]
pub struct FadeConfig {
    pub targets: Vec<FadeTarget>,
    pub duration_ms: u64,
    pub curve: FadeCurve,
}

pub enum FadeCommand {
    StartFade { config: FadeConfig },
    AbortAll,
    FinishNow,
    Subscribe { tx: mpsc::Sender<FadeEvent> },
}

#[derive(Debug, Clone)]
pub enum FadeEvent {
    FadeStarted,
    FadeCompleted,
    FadeAborted,
    ChannelOverride { group: i32, channel: i32 },
    ChannelCancelled { group: i32, channel: i32 },
}
```

- [ ] **Step 3: Create `src/fade/tick.rs`**

Move `TICK_HZ`, `MIN_SEND_DELTA_POS`, `OVERRIDE_THRESHOLD_POS`, and `ActiveChannel` from `src/fade/engine.rs`. Keep all methods and tests for `ActiveChannel` with the moved module. Import `crate::fade::curve::{FadeCurve, interpolate}` and `crate::fade::fader_law::db_to_pos`.

- [ ] **Step 4: Update `src/fade/mod.rs`**

Add module exports:

```rust
pub mod curve;
pub mod engine;
pub mod fader_law;
pub mod tick;
pub mod types;
```

Preserve any existing module lines not shown here.

- [ ] **Step 5: Update `src/fade/engine.rs` imports**

Use extracted modules:

```rust
use crate::fade::tick::{ActiveChannel, TICK_HZ};
use crate::fade::types::{FadeCommand, FadeConfig, FadeEvent};
```

Keep `FadeEngineHandle` and `spawn_engine` in `engine.rs`. If external callers currently use `lv1_scene_fade_utility::fade::engine::FadeConfig`, add compatibility re-exports in `engine.rs` during this refactor:

```rust
pub use crate::fade::types::{FadeConfig, FadeEvent, FadeTarget};
```

- [ ] **Step 6: Verify fade split**

Run:

```bash
cargo test fade --lib
```

Expected: all fade tests pass.

---

## Task 3: Split Tauri App State Into A Module Directory

**Files:**
- Create: `src-tauri/src/app_state/mod.rs`
- Create: `src-tauri/src/app_state/view.rs`
- Create: `src-tauri/src/app_state/shell.rs`
- Create: `src-tauri/src/app_state/capture.rs`
- Create: `src-tauri/src/app_state/events.rs`
- Create: `src-tauri/src/app_state/show_file_mapping.rs`
- Delete: `src-tauri/src/app_state.rs` after all content is moved
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Snapshot current Tauri tests**

Run:

```bash
cargo test -p lv1-scene-fade-utility-tauri
```

Expected: all Tauri package tests pass before moving code.

- [ ] **Step 2: Create `src-tauri/src/app_state/view.rs`**

Move only UI-facing serializable types from `app_state.rs`: `SceneSummary`, `ChannelSummary`, `FadeTarget`, `SceneFadeConfig`, `AppLogEntry`, `LogSource`, `LogSeverity`, `AppConnectionState`, `AppFadeState`, and `AppViewState`. Preserve derives, `serde(rename_all = "camelCase")`, field names, and visibility exactly.

- [ ] **Step 3: Create `src-tauri/src/app_state/shell.rs`**

Move `MAX_LOGS`, `RuntimeHandles`, `ShellState`, `ShellInner`, `Default` implementations, `ShellState::snapshot`, and helper functions that only build snapshots or provide timestamps. Import view types from `super::view`.

- [ ] **Step 4: Create `src-tauri/src/app_state/capture.rs`**

Move `ShellState` methods that mutate scene fade config, selected scene, listen mode, duration, and targets. Keep method names unchanged:

```rust
select_scene_config
set_scene_fade_enabled
set_listen_mode
set_scene_duration_ms
set_fade_target_enabled
remove_fade_target
```

Keep any helper functions used only by these methods in `capture.rs`.

- [ ] **Step 5: Create `src-tauri/src/app_state/events.rs`**

Move methods and helpers that apply `Lv1Event` and fade events, reconcile snapshots, and append log entries. Keep log severity/source behavior unchanged.

- [ ] **Step 6: Create `src-tauri/src/app_state/show_file_mapping.rs`**

Move methods and helpers that create, load, export, or map show files:

```rust
new_show_file
load_show_file_from_dto
export_show_file_for_save
current_show_file_path
show_file_from_inner
scene_config_from_show_file
```

- [ ] **Step 7: Create `src-tauri/src/app_state/mod.rs`**

Wire modules and public re-exports:

```rust
mod capture;
mod events;
mod shell;
mod show_file_mapping;
mod view;

pub use shell::{RuntimeHandles, ShellState};
pub use view::AppViewState;
```

Add extra `pub use` lines only for types directly imported by other modules.

- [ ] **Step 8: Update `src-tauri/src/commands.rs` imports**

Keep this import working:

```rust
use crate::app_state::{AppViewState, ShellState};
```

Adjust imports for `Lv1Event` if Task 1 moved it to `lv1::messages`.

- [ ] **Step 9: Verify Tauri state split**

Run:

```bash
cargo test -p lv1-scene-fade-utility-tauri
```

Expected: all Tauri package tests pass.

---

## Task 4: Split React `App.tsx` Into Components And Helpers

**Files:**
- Modify: `ui/src/App.tsx`
- Create: `ui/src/commands.ts`
- Create: `ui/src/components/Header.tsx`
- Create: `ui/src/components/ConnectionTab.tsx`
- Create: `ui/src/components/SceneTab.tsx`
- Create: `ui/src/components/LogsTab.tsx`
- Create: `ui/src/components/DurationInput.tsx`
- Create: `ui/src/components/StatusBadge.tsx`
- Create: `ui/src/components/ShowFileControls.tsx`
- Create: `ui/src/format.ts`

- [ ] **Step 1: Snapshot current UI verification**

Run:

```bash
npm --prefix ui run build
```

Expected: Vite build succeeds before moving code.

- [ ] **Step 2: Create `ui/src/format.ts`**

Move pure formatting helpers from `App.tsx`. Export each helper used by more than one component. Preserve output exactly.

- [ ] **Step 3: Create `ui/src/commands.ts`**

Move Tauri command helper functions that do not need component JSX. Keep `invoke<AppViewState>` typing and preserve error handling behavior. Export a small API that lets `App.tsx` run snapshot commands and refresh state.

- [ ] **Step 4: Create small shared components**

Move `StatusBadge`, `ShowFileControls`, and `DurationInput` into separate files. Preserve prop names and class names exactly. Export each component as a named export.

- [ ] **Step 5: Create tab components**

Move `ConnectionTab`, `SceneTab`, and `LogsTab` into separate files. Preserve props and JSX exactly except for imports.

- [ ] **Step 6: Create `Header` component**

Move the header JSX into `ui/src/components/Header.tsx`. Pass callbacks from `App.tsx`; do not let `Header` call Tauri directly.

- [ ] **Step 7: Reduce `App.tsx` to composition**

Keep only app-level state, `listen<AppViewState>("app-status-changed", ...)`, tab state, command wiring, and top-level layout in `App.tsx`.

- [ ] **Step 8: Verify UI split**

Run:

```bash
npm --prefix ui run build
```

Expected: Vite build succeeds.

---

## Task 5: Full Verification And Cleanup

**Files:**
- Modify only files touched by prior tasks if verification exposes import or formatting issues.

- [ ] **Step 1: Run full Rust tests**

Run:

```bash
cargo test --workspace
```

Expected: all Rust workspace tests pass.

- [ ] **Step 2: Run UI build**

Run:

```bash
npm --prefix ui run build
```

Expected: Vite build succeeds.

- [ ] **Step 3: Inspect diff for behavior changes**

Run:

```bash
git diff --stat
git diff -- src src-tauri/src ui/src
```

Expected: changes are module extraction, imports, and formatting only. No Tauri command names, event names, DTO serialized field names, fade constants, parser behavior, or UI text should change.

- [ ] **Step 4: Confirm no generated build output is included**

Run:

```bash
git status --short
```

Expected: source files and this plan may be modified. `target/`, `node_modules/`, and `ui/dist/` should not be staged or committed as part of this refactor.

---

## Self-Review

- Spec coverage: covers the recommended improvements for `src-tauri/src/app_state.rs`, `src/lv1/state.rs`, `src/fade/engine.rs`, and `ui/src/App.tsx`.
- Placeholder scan: no task uses TBD/TODO/fill-in placeholders; extraction steps name exact files and boundaries.
- Type consistency: moved type names preserve existing names; compatibility re-export is specified for fade engine public types to avoid unnecessary downstream changes.
- Scope check: this plan is intentionally behavior-preserving and excludes Phase 7 recall automation.

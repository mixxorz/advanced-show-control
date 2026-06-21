# Tauri Debug Smoke Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an isolated dev-only Tauri debug app that runs backend-observed LV1 hardware smoke tests and combines those results with frontend projector-output checks.

**Architecture:** Add a second Tauri app builder and binary that share the production runtime setup but register debug smoke-test commands. Backend smoke commands orchestrate named tests through the same lifecycle/actor paths as production commands, subscribe to `AppEventBus`, and capture DEBUG-level `tracing` events. The debug React app starts tests, watches `app-status-changed`, and reports pass only when backend events/traces and frontend projector expectations pass.

**Tech Stack:** Rust 2024, Tauri 2, Tokio broadcast/oneshot, `tracing`/`tracing-subscriber`, React 19, TypeScript, Vite.

## Global Constraints

- The production app must not register debug smoke-test commands.
- Debug commands must not directly write faders, directly emit `app-status-changed`, bypass lockout, bypass exact scene identity validation, or bypass generation guards.
- App actions under test must go through production lifecycle/actor command paths.
- Backend smoke assertions must use `AppEventBus` plus DEBUG-level `tracing` capture.
- UI-facing logs are not sufficient for backend smoke assertions.
- Frontend smoke assertions must use only `app-status-changed` snapshots and projected logs.
- Initial hardware suite requires two dedicated LV1 test scenes and one explicit test fader channel.
- Initial decreasing x-fade durations are `5000 ms`, `3000 ms`, `1000 ms`, and `500 ms`.
- Default fader tolerance is `0.5 dB`; default minimum movement threshold is `3.0 dB`.
- The smoke suite leaves the console in the final test state.
- Use the smallest correct change and preserve all safety-critical behavior.
- Do not commit unless the user explicitly asks for commits.

---

## File Structure

- Modify `src-tauri/src/ui/mod.rs`: extract shared runtime setup and keep production builder unchanged at the public interface.
- Create `src-tauri/src/ui/debug.rs`: debug app builder registering production commands plus debug smoke commands.
- Create `src-tauri/src/ui/debug/commands.rs`: expose debug smoke commands only through the debug builder module.
- Create `src-tauri/src/ui/debug/commands.rs`: Tauri command adapters for named smoke tests, registered only by the debug app builder.
- Create `src-tauri/src/smoke/mod.rs`: smoke module exports and shared result DTOs.
- Create `src-tauri/src/smoke/trace_capture.rs`: bounded DEBUG trace capture layer and event DTOs.
- Create `src-tauri/src/smoke/runner.rs`: backend smoke runner primitives for subscribing to events/traces, waiting for conditions, and sampling LV1 state.
- Create `src-tauri/src/smoke/tests.rs`: named smoke-test implementations.
- Modify `src-tauri/src/lib.rs`: export `smoke` and debug UI module as needed.
- Create `src-tauri/src/bin/advanced-show-control-debug.rs`: debug app binary.
- Create `src-tauri/tauri.debug.conf.json`: debug Tauri config.
- Modify `src-tauri/Cargo.toml`: ensure debug binary is discoverable if explicit bin metadata is needed.
- Create `ui/debug.html`: debug Vite HTML entry.
- Create `ui/src/debug/main.tsx`: debug React entrypoint.
- Create `ui/src/debug/SmokeDebugApp.tsx`: debug app composition and state ownership.
- Create `ui/src/debug/smokeTypes.ts`: frontend smoke DTOs matching backend result shapes.
- Create `ui/src/debug/smokeCommands.ts`: debug command wrappers.
- Create `ui/src/debug/projectorChecks.ts`: frontend projector expectation helpers.
- Create `ui/src/debug/SmokeTestPanel.tsx`: input form, test controls, result display.
- Modify `ui/package.json`: add `dev:debug` and `build:debug` scripts.
- Modify `ui/vite.config.ts`: support debug multi-page entry or debug build mode.

---

### Task 1: Shared Tauri Runtime Setup And Debug Builder

**Files:**
- Modify: `src-tauri/src/ui/mod.rs`
- Create: `src-tauri/src/ui/debug.rs`
- Create: `src-tauri/src/ui/debug/commands.rs`
- Create: `src-tauri/src/bin/advanced-show-control-debug.rs`
- Create: `src-tauri/tauri.debug.conf.json`
- Test: `src-tauri/src/ui/mod.rs` unit tests
- Test: `src-tauri/tests/manifest.rs`

**Interfaces:**
- Produces: `pub fn build_app() -> tauri::Builder<tauri::Wry>` unchanged.
- Produces: `pub fn build_debug_app() -> tauri::Builder<tauri::Wry>` in `advanced_show_control::ui::debug`.
- Produces: private `fn setup_shared_runtime<R: tauri::Runtime>(app: &mut tauri::App<R>, smoke_trace_capture: Option<SmokeTraceCapture>) -> Result<(), Box<dyn std::error::Error>>` or equivalent setup helper.

- [ ] **Step 1: Add failing builder tests**

Add tests in `src-tauri/src/ui/mod.rs`:

```rust
#[test]
fn debug_build_app_constructs_builder() {
    let _builder = super::debug::build_debug_app();
}

#[test]
fn production_builder_keeps_existing_command_exports() {
    let _ = super::commands::lifecycle::frontend_ready::<tauri::Wry>;
    let _ = super::commands::show::set_lockout;
    let _ = super::commands::scenes::recall_scene;
    let _ = super::commands::fade::abort_all_fades;
}
```

Add a test in `src-tauri/tests/manifest.rs`:

```rust
#[test]
fn debug_app_binary_is_declared_by_source_file() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/bin/advanced-show-control-debug.rs");
    assert!(path.exists(), "debug app binary source file must exist");
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo nextest run -p advanced-show-control ui::tests::debug_build_app_constructs_builder manifest::debug_app_binary_is_declared_by_source_file`

Expected: FAIL because `ui::debug` and `src/bin/advanced-show-control-debug.rs` do not exist.

- [ ] **Step 3: Extract shared runtime setup**

In `src-tauri/src/ui/mod.rs`, move the existing `.setup(...)` body into a helper and call it from `build_app()`:

```rust
fn setup_shared_runtime<R: tauri::Runtime>(
    app: &mut tauri::App<R>,
    smoke_trace_capture: Option<crate::smoke::SmokeTraceCapture>,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_bus = AppEventBus::default();
    let (show, show_task, show_peers) = build_show_actor(event_bus.clone());
    let lifecycle = AppLifecycle::new(event_bus, show.clone(), show_peers);
    show_task.spawn();
    let logging_runtime = logging::init_logging(app.handle(), smoke_trace_capture)?;
    app.manage(show);
    app.manage(lifecycle);
    app.manage(logging_runtime.guard);
    app.manage(logging_runtime.ui_logs);
    tracing::info!(event = "app_started", "Starting Advanced Show Control");
    Ok(())
}
```

Keep production `build_app()` registering only existing commands.

- [ ] **Step 4: Add debug builder and binary**

Create `src-tauri/src/ui/debug.rs`:

```rust
//! Debug-only Tauri app builder for hardware smoke tests.

use super::{commands as app_commands, setup_shared_runtime};
use crate::smoke::SmokeTraceCapture;

pub(crate) mod commands;

pub fn build_debug_app() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .setup(|app| {
            let capture = SmokeTraceCapture::new(2048);
            app.manage(capture.clone());
            setup_shared_runtime(app, Some(capture))
        })
        .invoke_handler(tauri::generate_handler![
            app_commands::lifecycle::frontend_ready,
            app_commands::show::refresh_lv1_discovery,
            app_commands::show::new_show_file,
            app_commands::show::open_show_file_dialog,
            app_commands::show::save_show_file,
            app_commands::show::save_show_file_as_dialog,
            app_commands::show::set_scene_duration_ms,
            app_commands::show::select_scene_config,
            app_commands::show::cue_scene,
            app_commands::scenes::recall_scene,
            app_commands::lifecycle::connect_lv1_system,
            app_commands::lifecycle::attempt_reconnect_lv1,
            app_commands::lifecycle::startup_auto_connect_lv1,
            app_commands::lifecycle::disconnect_lv1,
            app_commands::lifecycle::reconnect_timed_out,
            app_commands::fade::abort_all_fades,
            app_commands::show::store_scene_config,
            app_commands::show::set_channel_scoped,
            app_commands::show::set_all_channels_scoped,
            app_commands::show::set_scene_scope_faders_enabled,
            app_commands::show::set_scene_scope_pan_enabled,
            app_commands::show::set_lockout,
        ])
}
```

Add `pub mod debug;` in `src-tauri/src/ui/mod.rs`.

Production `build_app()` must call `setup_shared_runtime(app, None)` and must not import or register `ui::debug::commands`.

Create `src-tauri/src/bin/advanced-show-control-debug.rs`:

```rust
fn main() {
    advanced_show_control::ui::debug::build_debug_app()
        .run(tauri::generate_context!("tauri.debug.conf.json"))
        .expect("failed to run Advanced Show Control Debug");
}
```

Create `src-tauri/tauri.debug.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Advanced Show Control Debug",
  "version": "0.1.0",
  "identifier": "com.advancedshowcontrol.debug",
  "build": {
    "beforeDevCommand": "npm --prefix ui run dev:debug",
    "beforeBuildCommand": "npm --prefix ui run build:debug",
    "devUrl": "http://127.0.0.1:1421/debug.html",
    "frontendDist": "../ui/dist-debug"
  },
  "app": {
    "windows": [
      {
        "title": "Advanced Show Control Debug",
        "width": 1280,
        "height": 840,
        "minWidth": 1040,
        "minHeight": 700
      }
    ]
  },
  "bundle": {
    "active": false,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

- [ ] **Step 5: Run tests and build check**

Run: `cargo nextest run -p advanced-show-control ui::tests manifest::debug_app_binary_is_declared_by_source_file`

Expected: PASS.

Run: `cargo build -p advanced-show-control --bin advanced-show-control-debug`

Expected: PASS or fail only on missing frontend files referenced by config; if it fails on frontend files, continue with Task 6 before re-running.

---

### Task 2: DEBUG Trace Capture Layer

**Files:**
- Create: `src-tauri/src/smoke/mod.rs`
- Create: `src-tauri/src/smoke/trace_capture.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/smoke/trace_capture.rs`

**Interfaces:**
- Produces: `SmokeTraceEvent { timestamp_ms: u128, level: String, target: String, fields: Vec<SmokeTraceField> }`.
- Produces: `SmokeTraceCapture::new(capacity: usize) -> Self`.
- Produces: `SmokeTraceCapture::start_run(&self, test_id: impl Into<String>) -> SmokeTraceRun`.
- Produces: `SmokeTraceRun::finish(self) -> Vec<SmokeTraceEvent>`.
- Produces: `SmokeTraceLayer::new(capture: SmokeTraceCapture) -> SmokeTraceLayer`.

- [ ] **Step 1: Write failing trace capture tests**

Create `src-tauri/src/smoke/trace_capture.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    #[test]
    fn captures_debug_event_fields_during_active_run() {
        let capture = SmokeTraceCapture::new(16);
        let layer = SmokeTraceLayer::new(capture.clone());
        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        let run = capture.start_run("connection-test");
        tracing::debug!(event = "lv1_connect_requested", host = "127.0.0.1", port = 1234, "connecting");
        let events = run.finish();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].level, "DEBUG");
        assert!(events[0].has_field("event", "lv1_connect_requested"));
        assert!(events[0].has_field("host", "127.0.0.1"));
        assert!(events[0].has_field("port", "1234"));
    }

    #[test]
    fn ignores_events_when_no_run_is_active() {
        let capture = SmokeTraceCapture::new(16);
        let layer = SmokeTraceLayer::new(capture.clone());
        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::debug!(event = "outside_run", "outside");
        let run = capture.start_run("empty");
        let events = run.finish();

        assert!(events.is_empty());
    }

    #[test]
    fn keeps_only_capacity_latest_events() {
        let capture = SmokeTraceCapture::new(2);
        let layer = SmokeTraceLayer::new(capture.clone());
        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        let run = capture.start_run("bounded");
        tracing::debug!(event = "first");
        tracing::debug!(event = "second");
        tracing::debug!(event = "third");
        let events = run.finish();

        assert_eq!(events.len(), 2);
        assert!(events[0].has_field("event", "second"));
        assert!(events[1].has_field("event", "third"));
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo nextest run -p advanced-show-control smoke::trace_capture`

Expected: FAIL because types do not exist.

- [ ] **Step 3: Implement trace capture**

Create `src-tauri/src/smoke/mod.rs`:

```rust
pub mod trace_capture;
```

Modify `src-tauri/src/lib.rs`:

```rust
pub mod smoke;
```

Implement `src-tauri/src/smoke/trace_capture.rs`:

```rust
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SmokeTraceField {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SmokeTraceEvent {
    pub timestamp_ms: u128,
    pub level: String,
    pub target: String,
    pub fields: Vec<SmokeTraceField>,
}

impl SmokeTraceEvent {
    pub fn has_field(&self, name: &str, value: &str) -> bool {
        self.fields
            .iter()
            .any(|field| field.name == name && field.value == value)
    }
}

#[derive(Clone)]
pub struct SmokeTraceCapture {
    inner: Arc<Mutex<SmokeTraceCaptureInner>>,
}

struct SmokeTraceCaptureInner {
    capacity: usize,
    active_test_id: Option<String>,
    events: VecDeque<SmokeTraceEvent>,
}

pub struct SmokeTraceRun {
    capture: SmokeTraceCapture,
    test_id: String,
}

impl SmokeTraceCapture {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SmokeTraceCaptureInner {
                capacity: capacity.max(1),
                active_test_id: None,
                events: VecDeque::new(),
            })),
        }
    }

    pub fn start_run(&self, test_id: impl Into<String>) -> SmokeTraceRun {
        let test_id = test_id.into();
        let mut inner = self.inner.lock().expect("trace capture lock poisoned");
        inner.active_test_id = Some(test_id.clone());
        inner.events.clear();
        SmokeTraceRun {
            capture: self.clone(),
            test_id,
        }
    }

    fn push(&self, event: SmokeTraceEvent) {
        let mut inner = self.inner.lock().expect("trace capture lock poisoned");
        if inner.active_test_id.is_none() {
            return;
        }
        if inner.events.len() == inner.capacity {
            inner.events.pop_front();
        }
        inner.events.push_back(event);
    }
}

impl SmokeTraceRun {
    pub fn finish(self) -> Vec<SmokeTraceEvent> {
        let mut inner = self.capture.inner.lock().expect("trace capture lock poisoned");
        if inner.active_test_id.as_deref() == Some(self.test_id.as_str()) {
            inner.active_test_id = None;
        }
        inner.events.iter().cloned().collect()
    }
}

#[derive(Clone)]
pub struct SmokeTraceLayer {
    capture: SmokeTraceCapture,
}

impl SmokeTraceLayer {
    pub fn new(capture: SmokeTraceCapture) -> Self {
        Self { capture }
    }
}

impl<S> Layer<S> for SmokeTraceLayer
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = SmokeTraceVisitor { fields: Vec::new() };
        event.record(&mut visitor);
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        self.capture.push(SmokeTraceEvent {
            timestamp_ms,
            level: event.metadata().level().to_string(),
            target: event.metadata().target().to_string(),
            fields: visitor.fields,
        });
    }
}

struct SmokeTraceVisitor {
    fields: Vec<SmokeTraceField>,
}

impl tracing::field::Visit for SmokeTraceVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields.push(SmokeTraceField {
            name: field.name().to_string(),
            value: value.to_string(),
        });
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields.push(SmokeTraceField {
            name: field.name().to_string(),
            value: format!("{value:?}"),
        });
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo nextest run -p advanced-show-control smoke::trace_capture`

Expected: PASS.

---

### Task 3: Install Debug Trace Capture In Debug App Only

**Files:**
- Modify: `src-tauri/src/ui/debug.rs`
- Modify: `src-tauri/src/ui/mod.rs`
- Modify: `src-tauri/src/logging.rs`
- Modify: `src-tauri/src/smoke/mod.rs`
- Test: `src-tauri/src/ui/mod.rs`

**Interfaces:**
- Produces managed Tauri state: `SmokeTraceCapture` available only in debug app setup.
- Consumes: `SmokeTraceCapture` and `SmokeTraceLayer` from Task 2.

- [ ] **Step 1: Write failing tests for debug-only state installation**

Add to `src-tauri/src/ui/mod.rs` tests:

```rust
#[test]
fn smoke_trace_capture_type_is_exported_for_debug_state() {
    let capture = crate::smoke::SmokeTraceCapture::new(8);
    let _layer = crate::smoke::SmokeTraceLayer::new(capture);
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo nextest run -p advanced-show-control ui::tests::smoke_trace_capture_type_is_exported_for_debug_state`

Expected: FAIL because `SmokeTraceCapture` and `SmokeTraceLayer` are not re-exported from `smoke`.

- [ ] **Step 3: Re-export trace capture types**

Modify `src-tauri/src/smoke/mod.rs`:

```rust
pub mod trace_capture;

pub use trace_capture::{SmokeTraceCapture, SmokeTraceEvent, SmokeTraceField, SmokeTraceLayer};
```

- [ ] **Step 4: Install capture layer through the existing logging subscriber**

Modify `src-tauri/src/logging.rs` so `init_logging` accepts the optional smoke capture before it calls `try_init()`:

```rust
pub fn init_logging<R: Runtime>(
    app: &tauri::AppHandle<R>,
    smoke_trace_capture: Option<crate::smoke::SmokeTraceCapture>,
) -> Result<LoggingRuntime, Box<dyn Error>> {
    // existing log path, file/stdout/ui setup stays the same
    let smoke_layer = smoke_trace_capture
        .map(crate::smoke::SmokeTraceLayer::new)
        .with_filter(LevelFilter::DEBUG);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .with(ui_layer)
        .with(smoke_layer)
        .try_init()?;

    Ok(LoggingRuntime { guard, ui_logs: ui_tx })
}
```

Update production setup to call `logging::init_logging(app.handle(), None)?`.
Update any logging tests or helper call sites that call `logging::init_logging(app.handle())` to pass `None`.

Modify `src-tauri/src/ui/debug.rs` setup:

```rust
use crate::smoke::SmokeTraceCapture;

pub fn build_debug_app() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .setup(|app| {
            let capture = SmokeTraceCapture::new(2048);
            app.manage(capture.clone());
            setup_shared_runtime(app, Some(capture))
        })
        // existing invoke_handler
}
```

Do not call `try_init()` a second time from the debug builder. The smoke layer must be part of the one global subscriber initialized by `logging::init_logging`.

- [ ] **Step 5: Run focused tests**

Run: `cargo nextest run -p advanced-show-control ui::tests::smoke_trace_capture_type_is_exported_for_debug_state smoke::trace_capture`

Expected: PASS.

---

### Task 4: Smoke Result DTOs And Runner Primitives

**Files:**
- Create: `src-tauri/src/smoke/runner.rs`
- Modify: `src-tauri/src/smoke/mod.rs`
- Test: `src-tauri/src/smoke/runner.rs`

**Interfaces:**
- Produces: `SmokeStepResult`, `SmokeBackendResult`, `SmokeTestParams`, `SmokeTestChannel`.
- Produces: `async fn wait_for_event(...)` helper with timeout and lag handling.
- Produces: `fn summarize_app_event(event: &AppEvent) -> String` helper for result diagnostics.
- Produces: `fn pass_step(step: impl Into<String>, message: impl Into<String>) -> SmokeStepResult`.
- Produces: `fn fail_step(step: impl Into<String>, message: impl Into<String>, observed: serde_json::Value) -> SmokeStepResult`.

- [ ] **Step 1: Write failing DTO serialization tests**

Create `src-tauri/src/smoke/runner.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_backend_result_serializes_camel_case() {
        let result = SmokeBackendResult {
            ok: false,
            test_id: "connection".to_string(),
            started_at: "2026-06-21T00:00:00Z".to_string(),
            finished_at: "2026-06-21T00:00:01Z".to_string(),
            steps: vec![fail_step("connect", "not connected", serde_json::json!({"connection":"disconnected"}))],
            observed_events: vec!["Runtime".to_string()],
            observed_traces: vec![],
        };

        let json = serde_json::to_value(result).unwrap();

        assert_eq!(json["ok"], false);
        assert_eq!(json["testId"], "connection");
        assert_eq!(json["steps"][0]["step"], "connect");
        assert_eq!(json["observedEvents"][0], "Runtime");
    }
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo nextest run -p advanced-show-control smoke::runner::tests::smoke_backend_result_serializes_camel_case`

Expected: FAIL because DTOs do not exist.

- [ ] **Step 3: Implement DTOs**

Implement `src-tauri/src/smoke/runner.rs`:

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmokeStepResult {
    pub ok: bool,
    pub step: String,
    pub message: String,
    pub observed: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmokeBackendResult {
    pub ok: bool,
    pub test_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub steps: Vec<SmokeStepResult>,
    pub observed_events: Vec<String>,
    pub observed_traces: Vec<crate::smoke::SmokeTraceEvent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmokeTestChannel {
    pub group: i32,
    pub channel: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmokeTestParams {
    pub scene_a_id: String,
    pub scene_b_id: String,
    pub channel: SmokeTestChannel,
    pub tolerance_db: f64,
    pub minimum_movement_db: f64,
    pub timeout_ms: u64,
    pub sample_interval_ms: u64,
}

pub fn pass_step(step: impl Into<String>, message: impl Into<String>) -> SmokeStepResult {
    SmokeStepResult {
        ok: true,
        step: step.into(),
        message: message.into(),
        observed: None,
    }
}

pub fn fail_step(
    step: impl Into<String>,
    message: impl Into<String>,
    observed: Value,
) -> SmokeStepResult {
    SmokeStepResult {
        ok: false,
        step: step.into(),
        message: message.into(),
        observed: Some(observed),
    }
}
```

Modify `src-tauri/src/smoke/mod.rs`:

```rust
pub mod runner;
pub mod trace_capture;

pub use runner::{SmokeBackendResult, SmokeStepResult, SmokeTestChannel, SmokeTestParams};
pub use trace_capture::{SmokeTraceCapture, SmokeTraceEvent, SmokeTraceField, SmokeTraceLayer};
```

- [ ] **Step 4: Run tests**

Run: `cargo nextest run -p advanced-show-control smoke::runner`

Expected: PASS.

- [ ] **Step 5: Add event wait helper tests**

Add tests in `src-tauri/src/smoke/runner.rs`:

```rust
#[tokio::test]
async fn wait_for_event_returns_matching_event() {
    let bus = crate::runtime::events::AppEventBus::new(16);
    let mut rx = bus.subscribe();

    bus.publish_runtime_generation_changed(7);

    let found = wait_for_event(
        &mut rx,
        std::time::Duration::from_millis(50),
        |event| matches!(event, crate::runtime::events::AppEvent::Runtime(_)),
    )
    .await
    .unwrap();

    assert!(matches!(found, crate::runtime::events::AppEvent::Runtime(_)));
}

#[tokio::test]
async fn wait_for_event_times_out_without_match() {
    let bus = crate::runtime::events::AppEventBus::new(16);
    let mut rx = bus.subscribe();

    let err = wait_for_event(
        &mut rx,
        std::time::Duration::from_millis(10),
        |_| false,
    )
    .await
    .unwrap_err();

    assert!(err.contains("timed out"));
}
```

- [ ] **Step 6: Implement event wait helpers**

Add to `src-tauri/src/smoke/runner.rs`:

```rust
pub async fn wait_for_event(
    rx: &mut tokio::sync::broadcast::Receiver<crate::runtime::events::AppEvent>,
    timeout: std::time::Duration,
    mut predicate: impl FnMut(&crate::runtime::events::AppEvent) -> bool,
) -> Result<crate::runtime::events::AppEvent, String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err(format!("timed out after {} ms waiting for app event", timeout.as_millis()));
        }
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(event)) if predicate(&event) => return Ok(event),
            Ok(Ok(_event)) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_count))) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                return Err("app event bus closed while waiting for event".to_string());
            }
            Err(_) => {
                return Err(format!("timed out after {} ms waiting for app event", timeout.as_millis()));
            }
        }
    }
}

pub fn summarize_app_event(event: &crate::runtime::events::AppEvent) -> String {
    format!("{event:?}")
}
```

- [ ] **Step 7: Run event helper tests**

Run: `cargo nextest run -p advanced-show-control smoke::runner`

Expected: PASS.

---

### Task 5: Backend Smoke Command Adapters Skeleton

**Files:**
- Create: `src-tauri/src/ui/debug/commands.rs`
- Modify: `src-tauri/src/ui/debug.rs`
- Test: `src-tauri/src/ui/mod.rs`

**Interfaces:**
- Produces Tauri commands: `debug_smoke_run_connection_test`, `debug_smoke_run_scene_recall_test`, `debug_smoke_run_fade_starts_test`, `debug_smoke_run_fade_completes_test`, `debug_smoke_run_decreasing_xfade_test`, `debug_smoke_run_lockout_blocks_recall_test`.
- Consumes: `SmokeBackendResult`, `SmokeTestParams`.

- [ ] **Step 1: Write failing command export tests**

Add to `src-tauri/src/ui/mod.rs` tests:

```rust
#[test]
fn debug_command_adapter_exports_smoke_commands() {
    let _ = super::debug::commands::debug_smoke_run_connection_test::<tauri::Wry>;
    let _ = super::debug::commands::debug_smoke_run_scene_recall_test;
    let _ = super::debug::commands::debug_smoke_run_fade_starts_test;
    let _ = super::debug::commands::debug_smoke_run_fade_completes_test;
    let _ = super::debug::commands::debug_smoke_run_decreasing_xfade_test;
    let _ = super::debug::commands::debug_smoke_run_lockout_blocks_recall_test;
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo nextest run -p advanced-show-control ui::tests::debug_command_adapter_exports_smoke_commands`

Expected: FAIL because module does not exist.

- [ ] **Step 3: Add skeleton commands**

Create `src-tauri/src/ui/debug/commands.rs`:

```rust
use crate::connection_state::Lv1SystemIdentity;
use crate::lifecycle::AppLifecycle;
use crate::smoke::{SmokeBackendResult, SmokeTestParams};
use tauri::{AppHandle, Runtime, State};

#[tauri::command]
pub async fn debug_smoke_run_connection_test<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, AppLifecycle>,
    identity: Lv1SystemIdentity,
    timeout_ms: u64,
) -> Result<SmokeBackendResult, String> {
    crate::smoke::tests::run_connection_test(app, &lifecycle, identity, timeout_ms).await
}

#[tauri::command]
pub async fn debug_smoke_run_scene_recall_test(
    lifecycle: State<'_, AppLifecycle>,
    params: SmokeTestParams,
    target_scene_id: String,
) -> Result<SmokeBackendResult, String> {
    crate::smoke::tests::run_scene_recall_test(&lifecycle, params, target_scene_id).await
}

#[tauri::command]
pub async fn debug_smoke_run_fade_starts_test(
    lifecycle: State<'_, AppLifecycle>,
    params: SmokeTestParams,
) -> Result<SmokeBackendResult, String> {
    crate::smoke::tests::run_fade_starts_test(&lifecycle, params).await
}

#[tauri::command]
pub async fn debug_smoke_run_fade_completes_test(
    lifecycle: State<'_, AppLifecycle>,
    params: SmokeTestParams,
    expected_target_db: f64,
) -> Result<SmokeBackendResult, String> {
    crate::smoke::tests::run_fade_completes_test(&lifecycle, params, expected_target_db).await
}

#[tauri::command]
pub async fn debug_smoke_run_decreasing_xfade_test(
    lifecycle: State<'_, AppLifecycle>,
    params: SmokeTestParams,
) -> Result<SmokeBackendResult, String> {
    crate::smoke::tests::run_decreasing_xfade_test(&lifecycle, params).await
}

#[tauri::command]
pub async fn debug_smoke_run_lockout_blocks_recall_test(
    lifecycle: State<'_, AppLifecycle>,
    params: SmokeTestParams,
) -> Result<SmokeBackendResult, String> {
    crate::smoke::tests::run_lockout_blocks_recall_test(&lifecycle, params).await
}
```

Make `src-tauri/src/ui/debug.rs` declare `pub(crate) mod commands;` and include all six debug commands in the debug builder `invoke_handler`. Do not import this module from `src-tauri/src/ui/commands.rs`, and do not register these commands in production `build_app()`.

- [ ] **Step 4: Add placeholder test implementations that fail safely**

Create `src-tauri/src/smoke/tests.rs` and export it from `smoke/mod.rs`:

```rust
use crate::connection_state::Lv1SystemIdentity;
use crate::lifecycle::AppLifecycle;
use crate::smoke::{SmokeBackendResult, SmokeTestParams};

pub async fn run_connection_test(
    _app: tauri::AppHandle<impl tauri::Runtime>,
    _lifecycle: &AppLifecycle,
    _identity: Lv1SystemIdentity,
    _timeout_ms: u64,
) -> Result<SmokeBackendResult, String> {
    Err("debug smoke connection test is not implemented".to_string())
}

pub async fn run_scene_recall_test(
    _lifecycle: &AppLifecycle,
    _params: SmokeTestParams,
    _target_scene_id: String,
) -> Result<SmokeBackendResult, String> {
    Err("debug smoke scene recall test is not implemented".to_string())
}

pub async fn run_fade_starts_test(
    _lifecycle: &AppLifecycle,
    _params: SmokeTestParams,
) -> Result<SmokeBackendResult, String> {
    Err("debug smoke fade starts test is not implemented".to_string())
}

pub async fn run_fade_completes_test(
    _lifecycle: &AppLifecycle,
    _params: SmokeTestParams,
    _expected_target_db: f64,
) -> Result<SmokeBackendResult, String> {
    Err("debug smoke fade completes test is not implemented".to_string())
}

pub async fn run_decreasing_xfade_test(
    _lifecycle: &AppLifecycle,
    _params: SmokeTestParams,
) -> Result<SmokeBackendResult, String> {
    Err("debug smoke decreasing xfade test is not implemented".to_string())
}

pub async fn run_lockout_blocks_recall_test(
    _lifecycle: &AppLifecycle,
    _params: SmokeTestParams,
) -> Result<SmokeBackendResult, String> {
    Err("debug smoke lockout test is not implemented".to_string())
}
```

- [ ] **Step 5: Run tests**

Run: `cargo nextest run -p advanced-show-control ui::tests::debug_command_adapter_exports_smoke_commands`

Expected: PASS.

---

### Task 6: Debug Frontend Entry And Launch Scripts

**Files:**
- Create: `ui/debug.html`
- Create: `ui/src/debug/main.tsx`
- Create: `ui/src/debug/SmokeDebugApp.tsx`
- Create: `ui/src/debug/smokeTypes.ts`
- Create: `ui/src/debug/smokeCommands.ts`
- Modify: `ui/package.json`
- Modify: `ui/vite.config.ts`
- Test: `ui/src/debug/SmokeDebugApp.test.tsx`

**Interfaces:**
- Produces: `SmokeDebugApp` React component.
- Produces: `runConnectionTest`, `runSceneRecallTest`, `runFadeStartsTest`, `runFadeCompletesTest`, `runDecreasingXfadeTest`, `runLockoutBlocksRecallTest` wrappers.

- [ ] **Step 1: Write failing render test**

Create `ui/src/debug/SmokeDebugApp.test.tsx`:

```tsx
import { screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { render } from "../test/render";
import { SmokeDebugApp } from "./SmokeDebugApp";

describe("SmokeDebugApp", () => {
  it("renders the debug smoke test heading", () => {
    render(
      <SmokeDebugApp
        services={{
          frontendReady: vi.fn(async () => undefined),
          listenForAppStatus: vi.fn(async () => () => undefined),
        }}
      />,
    );

    expect(screen.getByRole("heading", { name: "LV1 Hardware Smoke Tests" })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run test to verify failure**

Run from `ui/`: `npm run test -- SmokeDebugApp.test.tsx`

Expected: FAIL because component does not exist.

- [ ] **Step 3: Add frontend debug files**

Create `ui/src/debug/smokeTypes.ts`:

```ts
export type SmokeStepResult = {
  ok: boolean;
  step: string;
  message: string;
  observed?: unknown;
};

export type SmokeBackendResult = {
  ok: boolean;
  testId: string;
  startedAt: string;
  finishedAt: string;
  steps: SmokeStepResult[];
  observedEvents: string[];
  observedTraces: unknown[];
};
```

Create `ui/src/debug/smokeCommands.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import type { Lv1SystemIdentity } from "../types";
import type { SmokeBackendResult } from "./smokeTypes";

export async function runConnectionTest(identity: Lv1SystemIdentity, timeoutMs: number) {
  return invoke<SmokeBackendResult>("debug_smoke_run_connection_test", {
    identity,
    timeoutMs,
  });
}
```

Create `ui/src/debug/SmokeDebugApp.tsx`:

```tsx
import { useEffect, useState } from "react";
import type { AppStatusListener } from "../AppRuntime";
import { disconnectedAppViewState, type AppViewState } from "../types";

export type SmokeDebugServices = {
  frontendReady: () => Promise<void>;
  listenForAppStatus: (listener: AppStatusListener) => Promise<() => void>;
};

export function SmokeDebugApp(props: { services: SmokeDebugServices }) {
  const [appState, setAppState] = useState<AppViewState>(disconnectedAppViewState);

  useEffect(() => {
    let cancelled = false;
    let unlisten: undefined | (() => void);

    async function start() {
      unlisten = await props.services.listenForAppStatus((snapshot) => {
        if (!cancelled) setAppState(snapshot);
      });
      await props.services.frontendReady();
    }

    void start();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [props.services]);

  return (
    <main className="min-h-screen bg-console-bg text-console-primary">
      <section className="mx-auto max-w-6xl p-6">
        <h1 className="text-2xl font-semibold">LV1 Hardware Smoke Tests</h1>
        <p className="mt-2 text-sm text-console-muted">
          Connection: {appState.connection}
        </p>
      </section>
    </main>
  );
}
```

Create `ui/src/debug/main.tsx`:

```tsx
import React from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppViewState } from "../types";
import "../index.css";
import { SmokeDebugApp } from "./SmokeDebugApp";

const services = {
  frontendReady: () => invoke<void>("frontend_ready"),
  listenForAppStatus: (listener: (snapshot: AppViewState) => void) =>
    listen<AppViewState>("app-status-changed", (event) => listener(event.payload)),
};

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <SmokeDebugApp services={services} />
  </React.StrictMode>,
);
```

Create `ui/debug.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Advanced Show Control Debug</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/debug/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 4: Add scripts and Vite debug mode**

Modify `ui/package.json` scripts:

```json
"dev:debug": "vite --host 127.0.0.1 --port 1421 --strictPort",
"build:debug": "VITE_DEBUG_ENTRY=1 vite build --outDir dist-debug"
```

Modify `ui/vite.config.ts` to use the debug HTML entry when `VITE_DEBUG_ENTRY=1`:

```ts
import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const debugEntry = process.env.VITE_DEBUG_ENTRY === "1";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: debugEntry ? 1421 : 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: debugEntry
    ? {
        rollupOptions: {
          input: resolve(__dirname, "debug.html"),
        },
      }
    : undefined,
});
```

- [ ] **Step 5: Run frontend checks**

Run from `ui/`: `npm run test -- SmokeDebugApp.test.tsx`

Expected: PASS.

Run from `ui/`: `npm run typecheck`

Expected: PASS.

---

### Task 7: Projector Expectation Helpers And Input-Gated Panel

**Files:**
- Create: `ui/src/debug/projectorChecks.ts`
- Create: `ui/src/debug/SmokeTestPanel.tsx`
- Modify: `ui/src/debug/SmokeDebugApp.tsx`
- Test: `ui/src/debug/projectorChecks.test.ts`
- Test: `ui/src/debug/SmokeTestPanel.test.tsx`

**Interfaces:**
- Produces: `projectorConnectionSteps(snapshot, expectedIdentity): SmokeStepResult[]`.
- Produces: `projectorFadeTransitionSteps(snapshots): SmokeStepResult[]`.
- Produces: `SmokeTestPanel` component with disabled run buttons until required inputs and acknowledgements exist.

- [ ] **Step 1: Write failing projector helper test**

Create `ui/src/debug/projectorChecks.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { disconnectedAppViewState, type AppViewState } from "../types";
import { projectorConnectionSteps } from "./projectorChecks";

describe("projectorConnectionSteps", () => {
  it("passes when projected connection and identity match", () => {
    const snapshot: AppViewState = {
      ...disconnectedAppViewState,
      connection: "connected",
      connectedLv1Identity: {
        uuid: "lv1",
        host: "lv1.local",
        address: "192.168.1.10",
        port: 12345,
      },
    };

    const steps = projectorConnectionSteps(snapshot, snapshot.connectedLv1Identity!);

    expect(steps.every((step) => step.ok)).toBe(true);
  });
});
```

- [ ] **Step 2: Implement projector helper**

Create `ui/src/debug/projectorChecks.ts`:

```ts
import type { AppViewState, Lv1SystemIdentity } from "../types";
import type { SmokeStepResult } from "./smokeTypes";

export function projectorConnectionSteps(
  snapshot: AppViewState,
  expected: Lv1SystemIdentity,
): SmokeStepResult[] {
  const identity = snapshot.connectedLv1Identity;
  return [
    {
      ok: snapshot.connection === "connected",
      step: "projector.connection",
      message:
        snapshot.connection === "connected"
          ? "Projector reported connected"
          : `Projector reported ${snapshot.connection}`,
      observed: { connection: snapshot.connection },
    },
    {
      ok:
        identity?.address === expected.address &&
        identity?.port === expected.port &&
        identity?.uuid === expected.uuid,
      step: "projector.connectedIdentity",
      message: "Projected identity matches selected LV1 identity",
      observed: { expected, identity },
    },
  ];
}
```

- [ ] **Step 3: Write failing panel gate test**

Create `ui/src/debug/SmokeTestPanel.test.tsx`:

```tsx
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { disconnectedAppViewState } from "../types";
import { render } from "../test/render";
import { SmokeTestPanel } from "./SmokeTestPanel";

describe("SmokeTestPanel", () => {
  it("keeps connection test disabled until danger acknowledgement is checked", async () => {
    render(
      <SmokeTestPanel
        appState={disconnectedAppViewState}
        onRunConnectionTest={vi.fn()}
      />,
    );

    const button = screen.getByRole("button", { name: "Run Connection Test" });
    expect(button).toBeDisabled();

    await userEvent.type(screen.getByLabelText("LV1 address"), "192.168.1.10");
    await userEvent.type(screen.getByLabelText("LV1 port"), "12345");
    await userEvent.click(screen.getByLabelText("I understand this can move hardware faders"));

    expect(button).toBeEnabled();
  });
});
```

- [ ] **Step 4: Implement panel**

Create `ui/src/debug/SmokeTestPanel.tsx` with controlled inputs for address, port, scene A, scene B, group, channel, tolerance, minimum movement, and acknowledgement. The initial button only needs to run connection test:

```tsx
import { useState } from "react";
import type { AppViewState, Lv1SystemIdentity } from "../types";

export function SmokeTestPanel(props: {
  appState: AppViewState;
  onRunConnectionTest: (identity: Lv1SystemIdentity) => void | Promise<void>;
}) {
  const [address, setAddress] = useState("");
  const [port, setPort] = useState("12345");
  const [ack, setAck] = useState(false);
  const canRunConnection = address.trim().length > 0 && Number(port) > 0 && ack;

  const identity: Lv1SystemIdentity = {
    uuid: null,
    host: null,
    address,
    port: Number(port),
  };

  return (
    <section className="mt-6 rounded-console-panel border border-console-line bg-console-panel p-4">
      <h2 className="text-lg font-semibold">Smoke Test Inputs</h2>
      <label className="mt-4 block text-sm">
        LV1 address
        <input className="mt-1 block w-full" value={address} onChange={(event) => setAddress(event.target.value)} />
      </label>
      <label className="mt-4 block text-sm">
        LV1 port
        <input className="mt-1 block w-full" value={port} onChange={(event) => setPort(event.target.value)} />
      </label>
      <label className="mt-4 flex items-center gap-2 text-sm">
        <input type="checkbox" checked={ack} onChange={(event) => setAck(event.target.checked)} />
        I understand this can move hardware faders
      </label>
      <button
        className="mt-4 rounded-console-control bg-accent-orange px-4 py-2 font-semibold text-console-bg disabled:opacity-50"
        disabled={!canRunConnection}
        onClick={() => void props.onRunConnectionTest(identity)}
      >
        Run Connection Test
      </button>
      <p className="mt-4 text-sm text-console-muted">Current projector connection: {props.appState.connection}</p>
    </section>
  );
}
```

Modify `SmokeDebugApp` to render `SmokeTestPanel`.

- [ ] **Step 5: Run frontend tests**

Run from `ui/`: `npm run test -- projectorChecks.test.ts SmokeTestPanel.test.tsx SmokeDebugApp.test.tsx`

Expected: PASS.

---

### Task 8: Connection Smoke Test Implementation

**Files:**
- Modify: `src-tauri/src/smoke/tests.rs`
- Modify: `src-tauri/src/smoke/runner.rs`
- Test: `src-tauri/src/smoke/tests.rs`

**Interfaces:**
- Consumes: `AppLifecycle::connect_lv1_system(app, identity)`, `AppEventBus` access may require adding `AppLifecycle::debug_smoke_event_bus(&self) -> AppEventBus`.
- Produces: `run_connection_test(...) -> Result<SmokeBackendResult, String>` with real checks.

- [ ] **Step 1: Add lifecycle event-bus accessor test**

Add a test in `src-tauri/src/lifecycle/mod.rs` tests:

```rust
#[tokio::test]
async fn debug_smoke_can_subscribe_to_lifecycle_event_bus() {
    let lifecycle = AppLifecycle::default();
    let bus = lifecycle.debug_smoke_event_bus();
    let mut rx = bus.subscribe();

    lifecycle.begin_connecting().await.unwrap();

    let event = rx.recv().await.unwrap();
    assert!(matches!(event, crate::runtime::events::AppEvent::Runtime(_)));
}
```

- [ ] **Step 2: Implement accessor**

Add to `AppLifecycle`:

```rust
pub fn debug_smoke_event_bus(&self) -> AppEventBus {
    self.event_bus.clone()
}
```

- [ ] **Step 3: Implement connection test event matching**

In `src-tauri/src/smoke/tests.rs`, implement `run_connection_test` to:

- Start trace run with test ID `connection`.
- Subscribe to `lifecycle.debug_smoke_event_bus()`.
- Call `lifecycle.connect_lv1_system(app, identity).await`. Do not manually decompose this into `abort_current_runtime`, `begin_connecting`, or `connect_to_identity`; the smoke test should exercise the same lifecycle entry point as the production command adapter.
- Collect events until `Lv1Event::Connected` for active generation and `ShowEvent::StateChanged` with connected identity.
- Finish trace run and assert trace fields include `event = "lv1_connect_requested"` and `event = "lv1_connected"`.

Use a helper result shape:

```rust
let ok = steps.iter().all(|step| step.ok);
Ok(SmokeBackendResult {
    ok,
    test_id: "connection".to_string(),
    started_at,
    finished_at,
    steps,
    observed_events,
    observed_traces,
})
```

- [ ] **Step 4: Add unit tests with synthetic events where possible**

Add tests for pure matching helpers rather than connecting hardware:

```rust
#[test]
fn connection_steps_pass_for_connected_event_and_trace() {
    let steps = connection_observation_steps(
        true,
        true,
        true,
        &[trace_event("lv1_connect_requested"), trace_event("lv1_connected")],
    );

    assert!(steps.iter().all(|step| step.ok));
}
```

- [ ] **Step 5: Run backend tests**

Run: `cargo nextest run -p advanced-show-control lifecycle::tests::debug_smoke_can_subscribe_to_lifecycle_event_bus smoke`

Expected: PASS.

---

### Task 9: Scene Recall, Fade Start, And Fade Completion Smoke Tests

**Files:**
- Modify: `src-tauri/src/smoke/tests.rs`
- Modify: `src-tauri/src/smoke/runner.rs`
- Test: `src-tauri/src/smoke/tests.rs`

**Interfaces:**
- Produces pure helper functions for matching scene recall, fade start, and fader movement observations.
- Consumes `Lv1Command::GetState` through `AppLifecycle::current_lv1()` or a new debug accessor if needed.

- [ ] **Step 1: Add lifecycle LV1 accessor if missing**

If `AppLifecycle` does not expose current LV1, add:

```rust
pub async fn debug_smoke_current_lv1(&self) -> Option<crate::lv1::Lv1ActorHandle> {
    self.current_lv1().await
}
```

Add a test that default lifecycle returns `None`.

- [ ] **Step 2: Add pure matcher tests**

In `src-tauri/src/smoke/tests.rs`, add tests:

```rust
#[test]
fn fade_completion_fails_when_manual_override_trace_is_present() {
    let steps = fade_completion_steps(
        true,
        true,
        -10.0,
        -10.2,
        0.5,
        &[trace_event("manual_override_detected")],
    );

    assert!(steps.iter().any(|step| !step.ok && step.step == "trace.noManualOverride"));
}

#[test]
fn fader_movement_passes_when_final_value_is_within_tolerance() {
    let steps = fader_movement_steps(-30.0, -10.0, -10.3, -30.0, -10.0, 0.5, 3.0, 8);

    assert!(steps.iter().all(|step| step.ok));
}
```

- [ ] **Step 3: Implement scene recall test**

Implement `run_scene_recall_test` to:

- Subscribe to events and start trace capture.
- Send `ScenesCommand::RecallScene` through `lifecycle.current_scene_recall_fader().await`.
- Wait for `Lv1Event::SceneChanged` matching target scene ID.
- Assert traces include scene recall request/ready/start requested events.
- Assert no `ScenesEvent::Blocked` or `ScenesEvent::Skipped` for the target.

- [ ] **Step 4: Implement fade starts test**

Implement `run_fade_starts_test` to:

- Recall from source to target using `ScenesCommand::RecallScene`.
- Wait for `ScenesEvent::Ready`, `ScenesEvent::StartRequested`, and `FadeEvent::FadeStarted`.
- Wait for at least one `Lv1Event::FaderChanged` for the configured channel in the expected direction if target values are available.
- Assert DEBUG traces include fade start event.

- [ ] **Step 5: Implement fade completes test**

Implement `run_fade_completes_test` to:

- Recall target scene.
- Sample LV1 state through `Lv1Command::GetState` every `sample_interval_ms`.
- Track start, min, max, final, sample count.
- Wait for `FadeEvent::FadeCompleted` or timeout.
- Assert final value within tolerance and no `FadeEvent::ChannelOverride` or manual override trace.

- [ ] **Step 6: Run targeted backend tests**

Run: `cargo nextest run -p advanced-show-control smoke`

Expected: PASS.

---

### Task 10: Decreasing X-Fade And Lockout Smoke Tests

**Files:**
- Modify: `src-tauri/src/smoke/tests.rs`
- Test: `src-tauri/src/smoke/tests.rs`

**Interfaces:**
- Produces decreasing x-fade runner with fixed durations `[5000, 3000, 1000, 500]`.
- Produces lockout runner that asserts blocked recall and no movement.

- [ ] **Step 1: Add pure sequence test**

Add test:

```rust
#[test]
fn decreasing_xfade_sequence_alternates_scenes() {
    let sequence = decreasing_xfade_sequence("0::A", "1::B");

    assert_eq!(sequence[0].duration_ms, 5000);
    assert_eq!(sequence[0].target_scene_id, "1::B");
    assert_eq!(sequence[1].duration_ms, 3000);
    assert_eq!(sequence[1].target_scene_id, "0::A");
    assert_eq!(sequence[2].duration_ms, 1000);
    assert_eq!(sequence[2].target_scene_id, "1::B");
    assert_eq!(sequence[3].duration_ms, 500);
    assert_eq!(sequence[3].target_scene_id, "0::A");
}
```

- [ ] **Step 2: Implement decreasing x-fade runner**

Implement runner to:

- For each duration, send `ShowCommand::SetSceneDuration` for the target scene.
- Recall target scene through `ScenesCommand::RecallScene`.
- Reuse fade completion observation logic.
- Fail immediately on any `FadeEvent::ChannelOverride`, manual override trace, or timeout.

- [ ] **Step 3: Add lockout matcher test**

Add test:

```rust
#[test]
fn lockout_steps_pass_when_blocked_without_fade_start() {
    let steps = lockout_steps(true, true, false, false, true);

    assert!(steps.iter().all(|step| step.ok));
}
```

- [ ] **Step 4: Implement lockout runner**

Implement runner to:

- Send `ShowCommand::SetLockout { enabled: true }`.
- Subscribe to events/traces.
- Attempt recall through `ScenesCommand::RecallScene`.
- Assert `ScenesEvent::Blocked` appears.
- Assert no LV1 scene change to blocked target, no fade start, no fader movement beyond tolerance, and DEBUG trace includes lockout block.
- Leave lockout enabled or disable it only if the debug UI explicitly asks. Initial implementation leaves app state as tested and reports it.

- [ ] **Step 5: Run tests**

Run: `cargo nextest run -p advanced-show-control smoke`

Expected: PASS.

---

### Task 11: Full Debug UI Test Controls And Combined Result Reporting

**Files:**
- Modify: `ui/src/debug/smokeCommands.ts`
- Modify: `ui/src/debug/SmokeTestPanel.tsx`
- Modify: `ui/src/debug/projectorChecks.ts`
- Test: `ui/src/debug/SmokeTestPanel.test.tsx`
- Test: `ui/src/debug/projectorChecks.test.ts`

**Interfaces:**
- Produces all six debug command wrappers.
- Produces combined result display with backend steps and projector steps.

- [ ] **Step 1: Add command wrapper tests through mocks**

Add tests that mock `@tauri-apps/api/core` and assert invoke names:

```ts
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => ({ ok: true, testId: "connection", startedAt: "", finishedAt: "", steps: [], observedEvents: [], observedTraces: [] })) }));

it("invokes decreasing xfade command", async () => {
  await runDecreasingXfadeTest(params);
  expect(invoke).toHaveBeenCalledWith("debug_smoke_run_decreasing_xfade_test", { params });
});
```

- [ ] **Step 2: Implement wrappers**

Add wrappers for all debug commands using exact command names from Task 5.

- [ ] **Step 3: Expand panel inputs and buttons**

Add inputs for scene A ID, scene B ID, group, channel, tolerance, minimum movement, timeout, and sample interval. Add buttons for each smoke test. Keep all buttons disabled until required fields and acknowledgements are present.

- [ ] **Step 4: Add combined result display**

Render backend and projector steps:

```tsx
function StepList(props: { title: string; steps: SmokeStepResult[] }) {
  return (
    <section>
      <h3>{props.title}</h3>
      <ul>
        {props.steps.map((step) => (
          <li key={step.step} data-ok={step.ok}>
            {step.ok ? "PASS" : "FAIL"}: {step.step} - {step.message}
          </li>
        ))}
      </ul>
    </section>
  );
}
```

- [ ] **Step 5: Run frontend tests**

Run from `ui/`: `npm run test -- smokeCommands SmokeTestPanel projectorChecks`

Expected: PASS.

---

### Task 12: Final Verification And Documentation Updates

**Files:**
- Modify: `docs/architecture.md` if debug app architecture needs mention.
- Modify: `docs/roadmap.md` if hardware smoke tests should be recorded in MVP support work.
- Verify: Rust and frontend checks.

**Interfaces:**
- Produces documented launch command and safety notes.

- [ ] **Step 1: Add docs note**

Add a short section to `docs/architecture.md` near Tauri command adapters:

```markdown
### Debug Smoke-Test App

The repository includes a development-only Tauri debug app for LV1 hardware smoke testing. It shares the production runtime setup and production command/actor paths, but registers additional debug smoke-test commands. Those commands observe `AppEventBus` facts and DEBUG-level `tracing` events; they do not emit `app-status-changed` or bypass production safety checks.
```

- [ ] **Step 2: Run Rust formatting and tests**

Run: `cargo fmt --all -- --check`

Expected: PASS.

Run: `cargo nextest run -p advanced-show-control smoke ui::tests manifest lifecycle::tests::debug_smoke_can_subscribe_to_lifecycle_event_bus`

Expected: PASS.

- [ ] **Step 3: Run frontend checks**

Run from `ui/`: `npm run format:check`

Expected: PASS.

Run from `ui/`: `npm run lint`

Expected: PASS.

Run from `ui/`: `npm run typecheck`

Expected: PASS.

Run from `ui/`: `npm run test`

Expected: PASS.

- [ ] **Step 4: Run debug build checks**

Run from repo root: `cargo build -p advanced-show-control --bin advanced-show-control-debug`

Expected: PASS.

Run from `ui/`: `npm run build:debug`

Expected: PASS.

- [ ] **Step 5: Manual launch smoke check without hardware**

Run in one terminal: `npm --prefix ui run dev:debug`

Expected: Vite serves debug app on `127.0.0.1:1421`.

Run in another terminal: `cd src-tauri && cargo run --bin advanced-show-control-debug`

Expected: Debug Tauri window opens and shows `LV1 Hardware Smoke Tests`.

Do not run hardware-moving smoke tests unless LV1 dedicated scenes and safe test channel are prepared.

---

## Self-Review Notes

- Spec coverage: The plan covers isolated debug app, DEBUG trace capture, backend event observation, frontend projector checks, all named tests, launch, safety constraints, and verification.
- Placeholder scan: The implementation tasks avoid unspecified placeholder instructions. Where implementation depends on Tauri subscriber installation behavior, the plan gives a concrete fallback: integrate the layer into logging setup while preserving production behavior.
- Type consistency: Backend result fields use `testId`, `startedAt`, `finishedAt`, `observedEvents`, and `observedTraces` via serde camelCase and matching TypeScript types.

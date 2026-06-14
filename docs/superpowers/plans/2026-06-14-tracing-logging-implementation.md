# Tracing Logging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace event-bus/UI-push logging with a single `tracing` pipeline that writes `DEBUG+` to JSONL files and stdout while projecting only `INFO+` to frontend state.

**Architecture:** Core and Tauri code emit structured `tracing` events with required `event` fields. Tauri installs three sinks: JSONL diagnostics, bracketed stdout, and a bounded UI sink that updates `ShellState` logs. `AppEventBus` remains only for runtime facts.

**Tech Stack:** Rust, Tokio, Tauri 2, `tracing`, `tracing-subscriber`, `tracing-appender`, React, TypeScript, Tailwind.

---

## File Map

- Modify `src-tauri/src/logging.rs`: Own tracing subscriber setup, file/stdout layers, UI log layer, test support types, and log directory setup.
- Modify `src-tauri/src/main.rs`: Initialize logging after Tauri app handle is available, register UI log sink with `ShellState`.
- Modify `src-tauri/src/diagnostics.rs`: Keep diagnostic log path helper if useful; remove manual append path if no longer used.
- Modify `src-tauri/src/app_state/view.rs`: Remove `LogSource` and `source` from `AppLogEntry`.
- Modify `src-tauri/src/app_state/shell.rs`: Replace `push_log(source, severity, message)` with internal append method used by tracing UI sink.
- Modify `src-tauri/src/app_state/events.rs`: Stop turning fact events into logs where logs belong at origin; keep state projection.
- Modify `src-tauri/src/app_state/logs.rs`: Remove scene recall log projection or downgrade to state-only projection.
- Modify `src-tauri/src/app_state/projection.rs`: Remove diagnostic log projection.
- Modify `src-tauri/src/app_state/show_file_mapping.rs`: Replace direct `push_log` with `tracing` at action outcomes.
- Modify `src-tauri/src/commands.rs`: Replace request/outcome logging per level plan, remove manual diagnostic file writes from shell projector, log command/action failures with command context.
- Modify `src/runtime/events.rs`: Remove `AppEvent::Diagnostic`, remove `AppEvent::CommandFailed`, replace lag helper with `tracing::debug!`.
- Modify `src/runtime/commands.rs`: Replace `publish_failure` with structured `tracing` logs; keep returning `Result`.
- Modify `src/lv1/state.rs`: Replace diagnostic event publishing with `tracing::debug!`; ensure OSC RX event logging emits address only.
- Modify `src/lv1/actor.rs`: add OSC TX DEBUG logging for writer task sends, write batches, set commands, and pong replies.
- Modify `src/lv1/tcp.rs`: add OSC TX DEBUG logging for `Lv1TcpClient::send` used by core-only CLI/probe paths.
- Modify `src/fade/actor.rs` and `src/fade/state.rs`: Log fade outcomes at origin; keep `FadeEvent` for runtime state facts.
- Modify `src/scene_recall/actor.rs`: Log recall blocked/skipped/ready/start-requested according to level plan; keep `SceneRecallEvent` only if still needed for state facts.
- Modify `ui/src/types.ts`: Remove `LogSource` and `source` from `AppLogEntry`.
- Modify `ui/src/components/LogsTab.tsx`: Remove source column, show severity and message.
- Modify `docs/architecture.md`: Update event bus and logging architecture.
- Modify tests under `src-tauri/src/app_state/*_tests.rs`, `src/runtime/commands.rs`, `src/runtime/events.rs`, `src/lv1/state.rs`, `src/fade/actor.rs`, `src/scene_recall/actor.rs`: replace old log transport assertions with tracing/UI sink/state fact assertions.

---

### Task 1: Remove Log Source From UI Model

**Files:**
- Modify: `src-tauri/src/app_state/view.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Modify: `ui/src/types.ts`
- Modify: `ui/src/components/LogsTab.tsx`
- Test: `src-tauri/src/app_state/events_tests.rs`

- [ ] **Step 1: Write failing Rust serialization test**

Add this test to `src-tauri/src/app_state/events_tests.rs`:

```rust
#[test]
fn app_log_entry_serializes_without_source() {
    let entry = super::view::AppLogEntry {
        id: 1,
        timestamp: "2026-06-14T12:34:56.789Z".to_string(),
        severity: super::view::LogSeverity::Info,
        message: "LV1 connected".to_string(),
    };

    let value = serde_json::to_value(entry).unwrap();

    assert!(value.get("source").is_none());
    assert_eq!(value["severity"], "info");
    assert_eq!(value["message"], "LV1 connected");
}
```

- [ ] **Step 2: Run failing test**

Run: `cargo nextest run -p advanced-show-control-tauri app_log_entry_serializes_without_source`

Expected: compile failure because `AppLogEntry` still requires `source`.

- [ ] **Step 3: Remove source from Rust view model**

In `src-tauri/src/app_state/view.rs`, change `AppLogEntry` and exports to:

```rust
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppLogEntry {
    pub id: u64,
    pub timestamp: String,
    pub severity: LogSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LogSeverity {
    Info,
    Warning,
    Error,
}
```

Remove the `LogSource` enum from the file and remove `LogSource` from `src-tauri/src/app_state/mod.rs` exports.

- [ ] **Step 4: Add source-free internal log append method**

In `src-tauri/src/app_state/shell.rs`, update imports to remove `LogSource` and add this method on `ShellState`:

```rust
pub async fn append_log(&self, severity: LogSeverity, message: String) -> AppViewState {
    let mut inner = self.inner.lock().await;
    inner.append_log(severity, message);
    drop(inner);
    self.snapshot().await
}
```

Replace `ShellInner::push_log` with:

```rust
pub(super) fn append_log(&mut self, severity: LogSeverity, message: String) {
    self.next_log_id += 1;
    let timestamp = crate::time::current_timestamp_millis();
    self.last_event_at = Some(timestamp.clone());
    self.logs.push_back(AppLogEntry {
        id: self.next_log_id,
        timestamp,
        severity,
        message,
    });
    while self.logs.len() > MAX_LOGS {
        self.logs.pop_front();
    }
}
```

Temporarily update existing call sites from `push_log(LogSource::X, severity, message)` to `append_log(severity, message)` or `inner.append_log(severity, message)` so the code compiles. Later tasks will remove most direct log calls.

- [ ] **Step 5: Remove source from TypeScript types**

In `ui/src/types.ts`, remove `LogSource` and change `AppLogEntry` to:

```ts
export type LogSeverity = "info" | "warning" | "error";

export type AppLogEntry = {
  id: number;
  timestamp: string;
  severity: LogSeverity;
  message: string;
};
```

- [ ] **Step 6: Update Logs tab layout**

In `ui/src/components/LogsTab.tsx`, use severity instead of source:

```tsx
appState.logs.map((entry) => (
  <div
    className="grid grid-cols-[9rem_6rem_1fr] gap-3 border-b border-slate-800 px-3 py-2 text-sm last:border-b-0"
    key={entry.id}
  >
    <span className="text-slate-500">{entry.timestamp}</span>
    <span className="uppercase text-slate-400">{entry.severity}</span>
    <span>{entry.message}</span>
  </div>
))
```

- [ ] **Step 7: Run checks**

Run: `cargo nextest run -p advanced-show-control-tauri app_log_entry_serializes_without_source`

Expected: PASS.

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/app_state ui/src
git commit -m "refactor: remove log source from UI logs"
```

---

### Task 2: Build Tauri Tracing Sinks

**Files:**
- Modify: `src-tauri/src/logging.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Test: `src-tauri/src/logging.rs`

- [ ] **Step 1: Write tests for UI filtering and message extraction**

Add tests to `src-tauri/src/logging.rs` under `#[cfg(test)]`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::LogSeverity;
    use tracing::Level;

    #[test]
    fn ui_severity_drops_debug() {
        assert_eq!(ui_severity(&Level::DEBUG), None);
    }

    #[test]
    fn ui_severity_maps_info_warn_error() {
        assert_eq!(ui_severity(&Level::INFO), Some(LogSeverity::Info));
        assert_eq!(ui_severity(&Level::WARN), Some(LogSeverity::Warning));
        assert_eq!(ui_severity(&Level::ERROR), Some(LogSeverity::Error));
    }

    #[test]
    fn event_requires_event_field_for_application_logs() {
        assert!(is_missing_event_field(&[("message", "hello")]));
        assert!(!is_missing_event_field(&[("event", "app_started"), ("message", "hello")]));
    }

    #[test]
    fn event_visitor_preserves_quoted_messages() {
        let mut visitor = EventVisitor::default();
        visitor.record_message("\"quoted\" message");
        assert_eq!(visitor.message.as_deref(), Some("\"quoted\" message"));
    }
}
```

- [ ] **Step 2: Run failing tests**

Run: `cargo nextest run -p advanced-show-control-tauri logging::tests`

Expected: compile failure because helper functions do not exist.

- [ ] **Step 3: Add logging sink types and helpers**

Replace `src-tauri/src/logging.rs` with a module shaped like this:

```rust
use std::path::PathBuf;

use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::mpsc;
use tracing::Level;
use tracing::field::{Field, Visit};
use tracing::subscriber::SetGlobalDefaultError;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Layer};

use crate::app_state::{LogSeverity, ShellState};

#[derive(Debug, Clone)]
pub struct UiLogEvent {
    pub severity: LogSeverity,
    pub message: String,
}

pub fn ui_severity(level: &Level) -> Option<LogSeverity> {
    match *level {
        Level::ERROR => Some(LogSeverity::Error),
        Level::WARN => Some(LogSeverity::Warning),
        Level::INFO => Some(LogSeverity::Info),
        Level::DEBUG | Level::TRACE => None,
    }
}

pub fn is_missing_event_field(fields: &[(impl AsRef<str>, impl AsRef<str>)]) -> bool {
    !fields.iter().any(|(name, value)| {
        name.as_ref() == "event" && !value.as_ref().trim().is_empty()
    })
}

pub fn init_logging<R: Runtime>(
    app: &AppHandle<R>,
    state: ShellState,
) -> Result<tracing_appender::non_blocking::WorkerGuard, String> {
    let diagnostics_path = crate::diagnostics::diagnostic_log_path(app);
    if let Some(parent) = diagnostics_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create diagnostics log directory: {err}"))?;
    }
    let file_appender = tracing_appender::rolling::never(
        diagnostics_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir),
        diagnostics_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("diagnostics.jsonl"),
    );
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let (ui_tx, ui_rx) = mpsc::channel::<UiLogEvent>(512);
    spawn_ui_log_projector(app.clone(), state, ui_rx);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));
    let stdout_layer = fmt::layer()
        .with_filter(LevelFilter::DEBUG)
        .with_target(true)
        .with_ansi(true)
        .event_format(BracketedFormatter);
    let file_layer = fmt::layer()
        .json()
        .with_writer(file_writer)
        .with_filter(LevelFilter::DEBUG);
    let ui_layer = UiLogLayer { tx: ui_tx }.with_filter(LevelFilter::INFO);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .with(ui_layer)
        .try_init()
        .map_err(|err| format_global_subscriber_error(err))?;

    app.manage(crate::diagnostics::DiagnosticLogPath(diagnostics_path));
    Ok(guard)
}

struct BracketedFormatter;

impl<S, N> FormatEvent<S, N> for BracketedFormatter
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        ctx: &fmt::FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let metadata = event.metadata();
        write!(
            writer,
            "[{}] [{}] [{}] ",
            crate::time::current_timestamp_millis(),
            metadata.level(),
            metadata.target()
        )?;
        ctx.field_format().format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

fn format_global_subscriber_error(err: SetGlobalDefaultError) -> String {
    format!("failed to initialize tracing subscriber: {err}")
}

struct UiLogLayer {
    tx: mpsc::Sender<UiLogEvent>,
}

impl<S> Layer<S> for UiLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        if event.metadata().target() == "advanced_show_control_tauri::logging::ui_sink" {
            return;
        }
        let Some(severity) = ui_severity(event.metadata().level()) else {
            return;
        };
        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);
        let message = visitor.message.unwrap_or_else(|| event.metadata().target().to_string());
        match self.tx.try_send(UiLogEvent { severity, message }) {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    target: "advanced_show_control_tauri::logging::ui_sink",
                    event = "ui_log_channel_full",
                    "UI log channel full; dropping UI log entry"
                );
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                tracing::error!(
                    target: "advanced_show_control_tauri::logging::ui_sink",
                    event = "ui_log_channel_closed",
                    "UI log channel closed; dropping UI log entry"
                );
            }
        }
    }
}

#[derive(Default)]
struct EventVisitor {
    message: Option<String>,
    event_name: Option<String>,
}

impl tracing::field::Visit for EventVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "message" => self.record_message(value),
            "event" => self.event_name = Some(value.to_string()),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "message" => self.record_message(&format!("{value:?}")),
            "event" => self.event_name = Some(format!("{value:?}")),
            _ => {}
        }
    }
}

impl EventVisitor {
    fn record_message(&mut self, value: &str) {
        self.message = Some(value.to_string());
    }
}

fn spawn_ui_log_projector<R: Runtime>(
    app: AppHandle<R>,
    state: ShellState,
    mut rx: mpsc::Receiver<UiLogEvent>,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(entry) = rx.recv().await {
            let snapshot = state.append_log(entry.severity, entry.message).await;
            if let Err(err) = app.emit("app-status-changed", &snapshot) {
                tracing::debug!(event = "app_status_emit_failed", error = %err, "Failed to emit app-status-changed");
            }
        }
    });
}
```

Use `try_init()`, not `init()`, so initialization failures are explicit. Unit tests should call helper functions directly and must not install the global subscriber.

Use `tracing_subscriber::Layer` and `tracing_subscriber::layer::SubscriberExt` imports so each layer can call `.with_filter(LevelFilter::...)`. Keep the behavior exactly as shown: stdout `DEBUG+`, file `DEBUG+`, UI `INFO+`.

- [ ] **Step 4: Initialize logging from Tauri app setup**

In `src-tauri/src/main.rs`, remove the early `logging::init_logging();` call before `Builder`. In `.setup`, after creating or getting `ShellState`, call:

```rust
let logging_guard = logging::init_logging(app.handle(), shell_state.clone())?;
app.manage(logging_guard);
tracing::info!(event = "app_started", "Starting Advanced Show Control");
```

If `ShellState` is currently created in `commands` or managed elsewhere, ensure the same `ShellState` clone is passed to `init_logging` and managed by Tauri.

- [ ] **Step 5: Run tests**

Run: `cargo nextest run -p advanced-show-control-tauri logging::tests`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/logging.rs src-tauri/src/main.rs src-tauri/src/app_state/shell.rs
git commit -m "feat: add tracing log sinks"
```

---

### Task 3: Add Replacement Safety Logs And Remove Event-Bus Log Transport

**Files:**
- Modify: `src/runtime/events.rs`
- Modify: `src/runtime/commands.rs`
- Modify: `src/lv1/state.rs`
- Modify: `src/fade/actor.rs` tests that expect `AppEvent::CommandFailed`
- Modify: `src/scene_recall/actor.rs`
- Modify: `src/fade/actor.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/app_state/projection.rs`
- Modify: tests in those modules

- [ ] **Step 1: Add safety-visible replacement log tests before deleting old transport**

Add tests that prove the new tracing UI sink can carry representative safety/user-visible messages. Put these in `src-tauri/src/logging.rs` tests using the `UiLogLayer` helpers from Task 2, without installing a global subscriber:

```rust
#[test]
fn safety_log_messages_are_ui_visible_levels() {
    assert_eq!(ui_severity(&tracing::Level::WARN), Some(LogSeverity::Warning));
    assert_eq!(ui_severity(&tracing::Level::ERROR), Some(LogSeverity::Error));
}

#[test]
fn safety_events_have_required_event_names() {
    assert!(!is_missing_event_field(&[("event", "scene_recall_blocked"), ("message", "Scene recall blocked")]));
    assert!(!is_missing_event_field(&[("event", "fade_aborted"), ("message", "Fade aborted")]));
    assert!(!is_missing_event_field(&[("event", "fade_manual_override"), ("message", "Fade manual override detected")]));
    assert!(!is_missing_event_field(&[("event", "command_failed"), ("message", "Command failed")]));
}

#[test]
fn ui_layer_projects_safety_warn_event() {
    use tracing_subscriber::layer::SubscriberExt;

    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let subscriber = tracing_subscriber::registry().with(UiLogLayer { tx });

    tracing::subscriber::with_default(subscriber, || {
        tracing::warn!(
            event = "scene_recall_blocked",
            scene = "4: Chorus",
            reason = "lockout enabled",
            "Scene recall blocked for 4: Chorus: lockout enabled"
        );
    });

    let entry = rx.try_recv().expect("ui layer should receive warning");
    assert_eq!(entry.severity, LogSeverity::Warning);
    assert_eq!(entry.message, "Scene recall blocked for 4: Chorus: lockout enabled");
}

#[test]
fn ui_layer_projects_command_failure_error_event() {
    use tracing_subscriber::layer::SubscriberExt;

    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let subscriber = tracing_subscriber::registry().with(UiLogLayer { tx });

    tracing::subscriber::with_default(subscriber, || {
        tracing::error!(
            event = "command_failed",
            command = "start_fade",
            error = "fade unavailable",
            "Command failed: start_fade: fade unavailable"
        );
    });

    let entry = rx.try_recv().expect("ui layer should receive error");
    assert_eq!(entry.severity, LogSeverity::Error);
    assert_eq!(entry.message, "Command failed: start_fade: fade unavailable");
}
```

Expected: PASS after Task 2. These tests are a migration guard: do not remove old log transport until the replacement sink can represent the safety events.

- [ ] **Step 2: Add origin tracing for safety/user-visible events**

Before removing any old event-bus log variants, add replacement origin logs:

In `src/scene_recall/actor.rs`, log blocked recalls at the decision point:

```rust
tracing::warn!(
    event = "scene_recall_blocked",
    scene = %scene_label,
    reason = %reason,
    "Scene recall blocked for {scene_label}: {reason}"
);
```

In `src/fade/actor.rs` or `src/fade/state.rs`, log safety-visible fade outcomes where the matching `FadeEvent` is emitted:

```rust
tracing::warn!(event = "fade_aborted", "Fade aborted");
tracing::warn!(event = "fade_manual_override", group, channel, parameter = ?parameter, "Fade manual override detected: group {group}, channel {channel}");
tracing::error!(event = "fade_write_failed", reason = %reason, "Fade write failed: {reason}");
```

In `src/runtime/commands.rs`, log command failures before returning to callers:

```rust
tracing::error!(
    event = "command_failed",
    command,
    error = %error,
    "Command failed: {command}: {error}"
);
```

Run: `cargo nextest run -p advanced-show-control fade scene_recall runtime::commands`

Expected: PASS. Do not proceed to Step 3 until this passes.

- [ ] **Step 3: Write compile-oriented event bus tests**

Update `src/runtime/events.rs` tests by deleting tests that publish `AppEvent::CommandFailed` or `AppEvent::Diagnostic`. Add:

```rust
#[tokio::test]
async fn app_event_bus_carries_runtime_facts() {
    let bus = AppEventBus::new(16);
    let mut rx = bus.subscribe();

    bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(SceneState {
        index: 7,
        name: "Chorus".to_string(),
    })));

    match rx.recv().await.unwrap() {
        AppEvent::Lv1(Lv1Event::SceneChanged(scene)) => {
            assert_eq!(scene.index, 7);
            assert_eq!(scene.name, "Chorus");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
```

- [ ] **Step 4: Remove log variants from `AppEvent`**

In `src/runtime/events.rs`, change enum to:

```rust
#[derive(Debug, Clone)]
pub enum AppEvent {
    Lv1(Lv1Event),
    Fade(FadeEvent),
    SceneRecall(crate::scene_recall::events::SceneRecallEvent),
}
```

Replace lag helper body with:

```rust
pub fn log_lagged_subscriber(name: &str, count: u64) {
    tracing::debug!(
        event = "event_subscriber_lagged",
        subscriber = name,
        missed_events = count,
        "Event subscriber lagged and missed {count} events"
    );
}
```

- [ ] **Step 5: Replace command failure publishing**

In `src/runtime/commands.rs`, replace `publish_failure` with:

```rust
fn log_failure(command: &str, result: &Result<(), AppCommandError>) {
    if let Err(error) = result {
        tracing::error!(
            event = "command_failed",
            command,
            error = %error,
            "Command failed: {command}: {error}"
        );
    }
}
```

Replace every `publish_failure(&self.event_bus, "command", &result);` with `log_failure("command", &result);`.

- [ ] **Step 6: Remove diagnostic event publishing**

In `src/lv1/state.rs`, replace `diagnose` with:

```rust
pub(super) fn diagnose(&mut self, message: impl Into<String>) {
    tracing::debug!(
        event = "lv1_diagnostic",
        "{}",
        message.into()
    );
}
```

Then migrate diagnostic OSC logging to the OSC-specific format in Task 7.

- [ ] **Step 7: Remove shell projector log-only arms**

In `src-tauri/src/commands.rs`, remove match arms for `AppEvent::CommandFailed` and `AppEvent::Diagnostic` in `spawn_shell_state_projector`. Remove `handle_diagnostic_event`. Remove manual `append_diagnostic` calls used only for logging delivery.

In `src-tauri/src/app_state/projection.rs`, remove the `AppEvent::Diagnostic` arm.

- [ ] **Step 8: Run targeted tests**

Run: `cargo nextest run -p advanced-show-control runtime::events runtime::commands`

Expected: PASS after updating tests that expected command failure events.

Run: `cargo nextest run -p advanced-show-control-tauri commands::tests app_state::events_tests`

Expected: PASS after deleting old diagnostic projection assertions.

Run: `cargo nextest run -p advanced-show-control fade`

Expected: PASS after updating `src/fade/actor.rs` tests that previously matched `AppEvent::CommandFailed`.

- [ ] **Step 9: Commit**

```bash
git add src/runtime src/lv1 src/fade src/scene_recall src-tauri/src/commands.rs src-tauri/src/app_state/projection.rs src-tauri/src/app_state/*tests.rs src-tauri/src/logging.rs
git commit -m "refactor: remove event bus log transport"
```

---

### Task 4: Move User-Visible Logs To Action Boundaries

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/app_state/show_file_mapping.rs`
- Modify: `src-tauri/src/app_state/events.rs`
- Modify: `src-tauri/src/app_state/logs.rs`
- Modify: `src/scene_recall/actor.rs`
- Modify: `src/fade/actor.rs`

- [ ] **Step 1: Add tests for request DEBUG and outcome INFO behavior**

Add or update tests in `src-tauri/src/app_state/events_tests.rs` to assert state projection does not create logs for scene list/topology updates:

```rust
#[tokio::test]
async fn scene_list_projection_does_not_append_ui_log() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;

    let snapshot = state
        .apply_lv1_event_for_generation(
            generation,
            &Lv1Event::SceneListChanged(vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }]),
        )
        .await
        .unwrap();

    assert!(snapshot.logs.iter().all(|entry| !entry.message.contains("Scene list updated")));
}
```

- [ ] **Step 2: Replace Tauri command request/outcome logs**

Use these patterns in `src-tauri/src/commands.rs`:

```rust
tracing::debug!(event = "lv1_discovery_requested", timeout_ms = timeout, "LV1 discovery requested");
tracing::info!(event = "lv1_discovery_started", timeout_ms = timeout, "Searching for LV1 systems on the network");
tracing::info!(event = "lv1_discovery_completed", system_count = systems.len(), elapsed_ms = latency_ms, "LV1 discovery completed: {} systems found", systems.len());
```

For failures returned from commands, log before returning:

```rust
tracing::error!(event = "show_file_save_failed", error = %err, "Show file save failed: {err}");
```

- [ ] **Step 3: Replace show file direct UI logs**

In `src-tauri/src/app_state/show_file_mapping.rs`, replace direct log appends with tracing outcomes:

```rust
tracing::info!(event = "show_file_created", "New show file created");
tracing::warn!(event = "show_file_scene_pruned", scene = %scene, "Deleted saved scene config during load: {scene}");
tracing::info!(event = "show_file_opened", "Show file loaded");
tracing::info!(event = "show_file_saved", "Show file saved");
```

- [ ] **Step 4: Remove fact projection logs that should move to origins**

In `src-tauri/src/app_state/events.rs`, keep state mutations for `Lv1Event` and `FadeEvent`, but remove `inner.append_log(...)` calls for:

- `SceneListChanged`
- `ChannelTopologyChanged`
- per-channel fade completion
- generic fade logs once fade origin logs are added

Do not remove state changes like `inner.fade_state = AppFadeState::Running`.

- [ ] **Step 5: Add scene recall logs at decision boundary**

In `src/scene_recall/actor.rs`, before publishing state facts, add logs:

```rust
tracing::warn!(event = "scene_recall_blocked", scene = %scene_label, reason = %reason, "Scene recall blocked for {scene_label}: {reason}");
tracing::debug!(event = "scene_recall_skipped", scene = %scene_label, reason = %reason, "Scene recall skipped for {scene_label}: {reason}");
tracing::debug!(event = "scene_recall_ready", scene = %scene_label, target_count = fade_config.targets.len(), "Scene recall ready for {scene_label}");
tracing::debug!(event = "scene_recall_start_requested", scene = %scene_label, "Scene recall start requested for {scene_label}");
```

Keep only `WARN` blocked events visible to UI.

- [ ] **Step 6: Add fade outcome logs at fade origin**

In `src/fade/actor.rs` or `src/fade/state.rs`, log outcomes where fade events are emitted:

```rust
tracing::info!(event = "fade_started", scene_index = config.scene.index, scene_name = %config.scene.name, duration_ms = config.duration_ms, target_count = config.targets.len(), "Fade started for {}: {} ({} targets, {} ms)", config.scene.index, config.scene.name, config.targets.len(), config.duration_ms);
tracing::info!(event = "fade_completed", "Fade completed");
tracing::warn!(event = "fade_aborted", "Fade aborted");
tracing::debug!(event = "fade_channel_completed", group, channel, parameter = ?parameter, "Fade channel completed: group {group}, channel {channel}");
tracing::warn!(event = "fade_manual_override", group, channel, parameter = ?parameter, "Fade manual override detected: group {group}, channel {channel}");
tracing::error!(event = "fade_write_failed", reason = %reason, "Fade write failed: {reason}");
```

- [ ] **Step 7: Run targeted tests**

Run: `cargo nextest run -p advanced-show-control-tauri app_state::events_tests app_state::show_file_mapping_tests`

Expected: PASS.

Run: `cargo nextest run -p advanced-show-control-tauri scene_recall`

Expected: PASS.

Run: `cargo nextest run -p advanced-show-control fade`

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/app_state src/scene_recall src/fade
git commit -m "refactor: move logs to action boundaries"
```

---

### Task 5: Add OSC Address DEBUG Logging

**Files:**
- Modify: `src/lv1/state.rs`
- Modify: `src/lv1/actor.rs`
- Modify: `src/lv1/tcp.rs`
- Test: `src/lv1/state.rs` tests
- Test: `src/lv1/actor.rs` tests
- Test: `src/lv1/tcp.rs` tests

- [ ] **Step 1: Locate RX and TX OSC paths**

Use code search for OSC handling:

Run: `rg "handle_message|OscMessage|/Ping|/Pong|write" src/lv1 src/osc.rs`

Expected: Confirm RX goes through `src/lv1/state.rs::handle_message`, actor TX goes through `src/lv1/actor.rs`, and CLI/probe TX goes through `src/lv1/tcp.rs::Lv1TcpClient::send`.

- [ ] **Step 2: Add RX logging helper**

In `src/lv1/state.rs`, add:

```rust
fn log_osc_rx(address: &str) {
    tracing::debug!(
        event = "osc_message",
        direction = "rx",
        osc_address = address,
        "OSC RX {address}"
    );
}
```

Call it at the top of `handle_message`:

```rust
pub(super) fn handle_message(state: &mut ActorState, msg: &crate::osc::OscMessage) {
    log_osc_rx(&msg.address);
    // existing handling follows
}
```

Do not include `msg.args` in the log.

- [ ] **Step 3: Add TX logging helper**

In `src/lv1/actor.rs`, add:

```rust
fn log_osc_tx(address: &str) {
    tracing::debug!(
        event = "osc_message",
        direction = "tx",
        osc_address = address,
        "OSC TX {address}"
    );
}
```

Call it for every outbound actor OSC message:

- `pong_for_ping` replies: log the pong address returned by `pong_for_ping`.
- `WriteBatch`: log each encoded address represented by the batch writes. For this task, log one `OSC TX` per write target before encoding or sending the batch.
- `SetGain`: log `/Set/Track/Out/Gain`.
- `SetPan`: log `/Set/Track/Pan`.
- `SetBalance`: log `/Set/Track/Pan/Balance`.
- `SetWidth`: log `/Set/Track/Pan/Width`.
- `SetMute`: log `/Set/Track/Out/Mute`.

Do not include args or values.

- [ ] **Step 4: Add CLI/probe TX logging**

In `src/lv1/tcp.rs`, add the same helper near `Lv1TcpClient::send`:

```rust
fn log_osc_tx(address: &str) {
    tracing::debug!(
        event = "osc_message",
        direction = "tx",
        osc_address = address,
        "OSC TX {address}"
    );
}
```

Call it at the top of `Lv1TcpClient::send`:

```rust
pub async fn send(&mut self, address: &str, args: &[OscArg]) -> TcpResult<()> {
    log_osc_tx(address);
    // existing send implementation follows
}
```

Do not log `args`.

- [ ] **Step 5: Add tests for address extraction helpers**

Add a pure helper in `src/lv1/actor.rs` so write batch address logging is testable without capturing tracing output:

```rust
fn write_parameter_address(write: &crate::lv1::commands::Lv1ParameterWrite) -> &'static str {
    match write.parameter {
        crate::lv1::commands::Lv1WriteParameter::FaderDb => "/Set/Track/Out/Gain",
        crate::lv1::commands::Lv1WriteParameter::Pan => "/Set/Track/Pan",
        crate::lv1::commands::Lv1WriteParameter::Balance => "/Set/Track/Pan/Balance",
        crate::lv1::commands::Lv1WriteParameter::Width => "/Set/Track/Pan/Width",
    }
}
```

Add this test to `src/lv1/actor.rs` tests:

```rust
#[test]
fn write_parameter_address_returns_only_osc_address() {
    use crate::lv1::commands::{Lv1ParameterWrite, Lv1WriteParameter};

    let write = Lv1ParameterWrite {
        group: 1,
        channel: 2,
        parameter: Lv1WriteParameter::FaderDb,
        value: -12.5,
    };

    assert_eq!(write_parameter_address(&write), "/Set/Track/Out/Gain");
}
```

This verifies the logging helper can emit address/type without using parameter values.

- [ ] **Step 6: Run targeted tests**

Run: `cargo nextest run -p advanced-show-control lv1`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/lv1
git commit -m "feat: log OSC traffic addresses"
```

---

### Task 6: Clean Up Obsolete Diagnostic Infrastructure

**Files:**
- Modify: `src-tauri/src/diagnostics.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/app_state/projection.rs`
- Modify: tests referencing diagnostics append/event transport

- [ ] **Step 1: Search old infrastructure**

Run: `rg "append_diagnostic|DiagnosticLogPath|AppEvent::Diagnostic|CommandFailed|push_log|eprintln!" src src-tauri/src ui/src`

Expected: Only intentional references remain after cleanup.

- [ ] **Step 2: Remove manual diagnostic append API**

After Task 3, `append_diagnostic` should have no production callers. Reduce `src-tauri/src/diagnostics.rs` to path management:

```rust
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, Runtime};

pub fn diagnostic_log_path<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("logs")
        .join(format!(
            "diagnostics-{started_at}-{}.jsonl",
            std::process::id()
        ))
}

#[derive(Clone)]
pub struct DiagnosticLogPath(pub PathBuf);
```

- [ ] **Step 3: Restrict direct UI log methods to the tracing sink**

Make `ShellState::append_log` `pub(crate)` and use it only from `src-tauri/src/logging.rs` tests or the UI log projector. `ShellInner::append_log` stays `pub(super)`.

- [ ] **Step 4: Remove obsolete tests**

Delete or rewrite tests named like:

- `diagnostic_event_updates_shell_state_log_and_snapshot`
- tests expecting `AppEvent::CommandFailed`
- tests expecting source values in UI logs

Replace with tests from earlier tasks.

- [ ] **Step 5: Run cleanup search again**

Run: `rg "AppEvent::Diagnostic|AppEvent::CommandFailed|LogSource|source: LogSource|append_diagnostic|eprintln!" src src-tauri/src ui/src`

Expected: No matches except unrelated CLI `src/main.rs` `eprintln!` output if that binary intentionally remains separate.

- [ ] **Step 6: Commit**

```bash
git add src src-tauri/src ui/src
git commit -m "refactor: clean up old logging infrastructure"
```

---

### Task 7: Update Architecture Docs

**Files:**
- Modify: `docs/architecture.md`
- Modify: `docs/superpowers/specs/2026-06-14-tracing-logging-design.md` only if implementation discovers a necessary correction

- [ ] **Step 1: Update architecture overview**

In `docs/architecture.md`, change the overview so logging is no longer implied to ride on `AppEventBus`:

```markdown
The runtime facts bus and logging pipeline are separate. `AppEventBus` broadcasts runtime facts used for state projection and policy decisions. Logging uses `tracing`; Tauri installs the desktop sinks for diagnostic files, stdout, and frontend log state.
```

- [ ] **Step 2: Update bus contracts**

Add to Bus Contracts:

```markdown
`AppEventBus` must not carry log-only events. If something is only a diagnostic or user-facing log, emit a `tracing` event instead.
```

- [ ] **Step 3: Add logging flow diagram**

Add:

```text
Core + Tauri tracing events
  -> Tauri tracing subscriber
    -> DEBUG+ JSONL diagnostic file
    -> DEBUG+ stdout
    -> INFO+ frontend app state
```

- [ ] **Step 4: Run docs diff review**

Run: `git diff -- docs/architecture.md`

Expected: Diff states fact bus and tracing pipeline clearly without changing runtime ownership claims.

- [ ] **Step 5: Commit**

```bash
git add docs/architecture.md
git commit -m "docs: document tracing logging architecture"
```

---

### Task 8: Full Verification

**Files:**
- No planned edits unless verification exposes issues.

- [ ] **Step 1: Run Rust formatting check**

Run: `cargo fmt --all -- --check`

Expected: PASS. If it fails, run `cargo fmt --all`, review the diff, and commit formatting with the relevant fix commit rather than a standalone unrelated commit if possible.

- [ ] **Step 2: Run Rust tests**

Run: `cargo nextest run --workspace`

Expected: PASS.

- [ ] **Step 3: Run Rust linting**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 4: Run frontend typecheck**

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 5: Run frontend build**

Run: `npm run build`

Expected: PASS.

- [ ] **Step 6: Manual smoke check logs**

Run the Tauri app or targeted startup path used by the project. Confirm:

- stdout uses bracketed formatting
- diagnostic JSONL file is created
- `DEBUG` logs appear in file/stdout but not UI
- `INFO` startup/discovery/connect logs appear in UI
- UI log entries have no source field

- [ ] **Step 7: Final cleanup search**

Run: `rg "AppEvent::Diagnostic|AppEvent::CommandFailed|LogSource|append_diagnostic|source: LogSource" src src-tauri/src ui/src`

Expected: no matches.

Run: `rg "tracing::(debug|info|warn|error)!\(" src src-tauri/src`

Expected: application log call sites include `event = "..."`. OSC log calls use `event = "osc_message"` and do not include args.

- [ ] **Step 8: Commit any verification fixes**

If verification required fixes:

```bash
git add <fixed files>
git commit -m "fix: complete tracing logging migration"
```

If no fixes were needed, do not create an empty commit.

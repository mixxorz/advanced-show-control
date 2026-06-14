# Tracing Logging Design

## Goal

Replace the app's log transport with `tracing`. Runtime modules should emit logs through `tracing::{debug, info, warn, error}`. Tauri owns subscriber setup and decides where log events go.

The `AppEventBus` remains a runtime facts bus. It should not carry log-only events.

## Scope

- Use `tracing` as the only application logging API.
- Write `DEBUG` and above to diagnostic JSONL log files.
- Print `DEBUG` and above to stdout.
- Project only `INFO`, `WARN`, and `ERROR` events into frontend state.
- Remove log source from UI log state and frontend types.
- Replace existing event-bus diagnostic logging with tracing events.
- Keep runtime facts such as LV1 state, fade state, and scene recall state on `AppEventBus`.

## Non-Goals

- No backwards compatibility for the old log shape.
- No duplicate source taxonomy for UI logs.
- No `Debug` severity in frontend state.
- No durable event store or replay.

## Ownership Boundary

The core crate owns log call sites only. It does not know about diagnostic files, stdout formatting, frontend state, Tauri events, or UI log formatting.

Tauri owns logging delivery:

- initialize `tracing` subscribers
- create and manage the diagnostic log file writer
- install the stdout sink
- install the frontend state sink
- filter which levels are projected to UI

Core-only binaries and tests may initialize tracing separately or not at all.

## Log Flow

```text
core + tauri tracing events
  -> Tauri tracing subscriber
    -> DEBUG+ JSONL diagnostic file
    -> DEBUG+ stdout
    -> INFO+ frontend app state
```

`DEBUG` events are never represented in frontend state.

## Event Shape

Every important app log should include a stable event name and a complete human-readable message.

Example:

```rust
tracing::warn!(
    event = %LogEvent::SceneRecallBlocked,
    scene = %scene_label,
    reason = %reason,
    "Scene recall blocked for {scene_label}: {reason}"
);
```

The message must be complete enough to show directly in the UI. Structured fields are for diagnostics, searching, and filtering in JSONL logs.

The Rust tracing target remains available in file/stdout logs to identify the module that emitted the event. There is no separate app log source field.

## Log Event Names

Use an enum for stable event names where the event is app-owned and significant.

Suggested shape:

```rust
pub enum LogEvent {
    AppStarted,
    Lv1DiscoveryStarted,
    Lv1Connected,
    Lv1Disconnected,
    SceneChanged,
    SceneListChanged,
    SceneRecallBlocked,
    SceneRecallSkipped,
    SceneRecallReady,
    SceneRecallStartRequested,
    FadeStarted,
    FadeCompleted,
    FadeAborted,
    FadeWriteFailed,
    CommandFailed,
    ShowFileOpened,
    ShowFileSaved,
}
```

`Display` should serialize values to stable snake_case strings, such as `scene_recall_blocked`.

Small one-off technical debug logs may omit `event` if they are not useful as stable diagnostic events.

## Diagnostic JSONL Shape

Diagnostic files are JSONL and receive `DEBUG+`.

Each entry should include:

- timestamp
- level
- target
- event, when provided
- message
- structured fields

Example:

```json
{
  "timestamp": "2026-06-14T12:34:56.789Z",
  "level": "WARN",
  "target": "advanced_show_control::scene_recall::actor",
  "event": "scene_recall_blocked",
  "message": "Scene recall blocked for 4: Chorus: lockout enabled",
  "fields": {
    "scene": "4: Chorus",
    "reason": "lockout enabled",
    "generation": 7
  }
}
```

## Stdout Shape

Stdout receives `DEBUG+` and may use human-readable text formatting. It does not need to match the JSONL file shape exactly.

Example:

```text
2026-06-14T12:34:56.789Z WARN advanced_show_control::scene_recall::actor scene_recall_blocked Scene recall blocked for 4: Chorus: lockout enabled scene="4: Chorus" reason="lockout enabled"
```

## Frontend UI Shape

Frontend state receives only `INFO+`.

Remove log source from the Rust view model and TypeScript mirror types.

Rust shape:

```rust
pub struct AppLogEntry {
    pub id: u64,
    pub timestamp: String,
    pub severity: LogSeverity,
    pub message: String,
}

pub enum LogSeverity {
    Info,
    Warning,
    Error,
}
```

TypeScript shape:

```ts
export type LogSeverity = "info" | "warning" | "error";

export type AppLogEntry = {
  id: number;
  timestamp: string;
  severity: LogSeverity;
  message: string;
};
```

There is no `LogSource` in the UI model. There is no `debug` frontend severity.

## Frontend Projection

The Tauri UI tracing layer converts tracing events into `AppLogEntry` values.

Rules:

- Drop events below `INFO`.
- Map `INFO` to `LogSeverity::Info`.
- Map `WARN` to `LogSeverity::Warning`.
- Map `ERROR` to `LogSeverity::Error`.
- Use the formatted tracing message as `message`.
- Preserve the existing capped in-memory log list behavior.
- Emit `app-status-changed` after a UI-visible log entry is added.

The projection path should avoid blocking tracing call sites. If a channel is needed between the tracing layer and async shell state, it should be bounded and should fail visibly in stdout/file diagnostics rather than blocking runtime work.

## Event Bus Changes

Remove log-only variants from `AppEvent`.

Expected changes:

- Remove `AppEvent::Diagnostic`.
- Stop using `AppEvent::CommandFailed` as a UI log transport.
- Log command failures at the command boundary with `tracing::error!` or `tracing::warn!`, depending on severity.
- Keep `AppEvent::Lv1`, `AppEvent::Fade`, and `AppEvent::SceneRecall` as runtime facts for state projection and policy decisions.

If a command failure needs to affect application state, model that as a state/fact event rather than a log event.

## Logging Level Guidance

- `DEBUG`: protocol details, internal decisions, noisy diagnostics, subscriber lag details, state counts, low-level write/drop information.
- `INFO`: normal user-relevant lifecycle events, scene changes, fade starts/completions, show file open/save, lockout toggles.
- `WARN`: safety blocks, skipped unsafe work, disconnects, lag, manual override, recoverable failures.
- `ERROR`: command failures, unrecoverable runtime setup failures, failed file writes that prevent diagnostics or user-requested persistence.

Safety-related blocks must be at least `WARN` so they appear in UI state.

## Tests

Add or update tests for:

- `DEBUG` tracing events are written to file/stdout sinks but not projected into UI state.
- `INFO`, `WARN`, and `ERROR` tracing events are projected into UI state with the existing severities.
- UI `AppLogEntry` no longer serializes a source field.
- `AppEvent::Diagnostic` is removed and diagnostics no longer depend on event bus delivery.
- Existing scene recall, fade, and LV1 facts still update shell state through the event bus.

## Documentation Updates

Update architecture docs to state:

- `AppEventBus` carries runtime facts only.
- Logging is handled by `tracing`.
- Tauri owns logging subscribers and output sinks.
- Core emits tracing events without owning delivery.

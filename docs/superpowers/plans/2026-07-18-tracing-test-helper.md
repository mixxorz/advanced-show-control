# Reusable Tracing Test Helper Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete issue #38 by adding isolated shared tracing assertions for Rust actor tests without changing production logging.

**Architecture:** Add one crate-private `#[cfg(test)]` module that installs a scoped thread-local tracing subscriber and stores structured events in a per-instance buffer. Prove its field, level, message, and isolation behavior directly, then migrate one current-thread lifecycle actor test away from its process-global subscriber.

**Tech Stack:** Rust 2024, `tracing`, `tracing-subscriber`, Tokio, cargo-nextest

## Global Constraints

- This is the first plan in the approved order: `#38 -> #57 -> #53 -> #59 -> #58 -> #56 -> #55`.
- Keep the helper test-only and crate-private; production subscriber setup in `src-tauri/src/logging.rs` must not change.
- Capture every structured field, the stable `event` field, `tracing::Level`, and the complete human-readable `message`.
- Do not use `set_global_default`; collector instances must not leak events across tests.
- Before advancing to issue #57, run one successful `make smoke` and inspect `logs/debug-smoke-report.txt`.

---

### Task 1: Add And Demonstrate The Shared Collector

**Files:**
- Create: `src-tauri/src/test_support.rs`
- Modify: `src-tauri/src/lib.rs:1-16`
- Modify: `src-tauri/src/lifecycle/mod.rs:885-946,1811-1863`
- Test: `src-tauri/src/test_support.rs`
- Test: `src-tauri/src/lifecycle/mod.rs`

**Interfaces:**
- Consumes: `tracing::subscriber::set_default`, `tracing_subscriber::registry::Registry`, and `tracing_subscriber::Layer`.
- Produces: `TracingCapture::new()`, `TracingCapture::install() -> tracing::dispatcher::DefaultGuard`, `TracingCapture::with_default`, `TracingCapture::events()`, and `TracingCapture::matching(event, level)`.

- [ ] **Step 1: Register test support and write failing collector tests**

Add this declaration to `src-tauri/src/lib.rs`:

```rust
#[cfg(test)]
pub(crate) mod test_support;
```

Create `src-tauri/src/test_support.rs` with imports and tests that refer to the not-yet-implemented API:

```rust
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::{Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::{LookupSpan, Registry};
use tracing_subscriber::Layer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CapturedTracingEvent {
    pub(crate) level: Level,
    pub(crate) event: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) fields: BTreeMap<String, String>,
}

#[derive(Clone, Default)]
pub(crate) struct TracingCapture {
    events: Arc<Mutex<Vec<CapturedTracingEvent>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_level_event_complete_message_and_structured_fields() {
        let capture = TracingCapture::new();
        capture.with_default(|| {
            tracing::warn!(
                event = "scene_recall_blocked",
                scene = "4: Chorus",
                attempt = 3_u64,
                lockout_enabled = true,
                "Scene recall blocked for 4: Chorus: lockout enabled"
            );
        });

        let events = capture.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].level, Level::WARN);
        assert_eq!(events[0].event.as_deref(), Some("scene_recall_blocked"));
        assert_eq!(
            events[0].message.as_deref(),
            Some("Scene recall blocked for 4: Chorus: lockout enabled")
        );
        assert_eq!(events[0].fields.get("scene").map(String::as_str), Some("4: Chorus"));
        assert_eq!(events[0].fields.get("attempt").map(String::as_str), Some("3"));
        assert_eq!(events[0].fields.get("lockout_enabled").map(String::as_str), Some("true"));
    }

    #[test]
    fn filters_by_stable_event_and_level() {
        let capture = TracingCapture::new();
        capture.with_default(|| {
            tracing::warn!(event = "scene_recall_blocked", "blocked");
            tracing::info!(event = "scene_recall_blocked", "blocked");
            tracing::warn!(event = "scene_recall_skipped", "skipped");
        });

        let matching = capture.matching("scene_recall_blocked", Level::WARN);
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].message.as_deref(), Some("blocked"));
    }

    #[test]
    fn scoped_collectors_do_not_leak_between_threads() {
        let first = TracingCapture::new();
        let second = TracingCapture::new();
        std::thread::scope(|scope| {
            let first = first.clone();
            scope.spawn(move || first.with_default(|| tracing::info!(event = "first", "first")));
            let second = second.clone();
            scope.spawn(move || second.with_default(|| tracing::warn!(event = "second", "second")));
        });

        assert_eq!(first.matching("first", Level::INFO).len(), 1);
        assert!(first.events().iter().all(|event| event.event.as_deref() != Some("second")));
        assert_eq!(second.matching("second", Level::WARN).len(), 1);
        assert!(second.events().iter().all(|event| event.event.as_deref() != Some("first")));
    }
}
```

- [ ] **Step 2: Run the tests to verify the API is incomplete**

Run:

```bash
cargo nextest run -p advanced-show-control test_support::tests
```

Expected: compilation fails because `TracingCapture::new`, `with_default`, `events`, and `matching` do not exist.

- [ ] **Step 3: Implement the minimal collector**

Insert this implementation before the test module in `src-tauri/src/test_support.rs`:

```rust
impl TracingCapture {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn install(&self) -> tracing::dispatcher::DefaultGuard {
        tracing::subscriber::set_default(Registry::default().with(self.clone()))
    }

    pub(crate) fn with_default<T>(&self, run: impl FnOnce() -> T) -> T {
        let _guard = self.install();
        run()
    }

    pub(crate) fn events(&self) -> Vec<CapturedTracingEvent> {
        self.events.lock().expect("tracing capture lock poisoned").clone()
    }

    pub(crate) fn matching(&self, event: &str, level: Level) -> Vec<CapturedTracingEvent> {
        self.events()
            .into_iter()
            .filter(|captured| captured.level == level && captured.event.as_deref() == Some(event))
            .collect()
    }
}

impl<S> Layer<S> for TracingCapture
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        let captured = CapturedTracingEvent {
            level: *event.metadata().level(),
            event: visitor.fields.get("event").cloned(),
            message: visitor.fields.get("message").cloned(),
            fields: visitor.fields,
        };
        self.events
            .lock()
            .expect("tracing capture lock poisoned")
            .push(captured);
    }
}

#[derive(Default)]
struct FieldVisitor {
    fields: BTreeMap<String, String>,
}

impl Visit for FieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields.insert(field.name().to_string(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields.insert(field.name().to_string(), format!("{value:?}"));
    }
}
```

- [ ] **Step 4: Run collector tests green**

Run:

```bash
cargo nextest run -p advanced-show-control test_support::tests
```

Expected: all three `test_support::tests` pass.

- [ ] **Step 5: Migrate one lifecycle actor logging test**

In `accepted_connect_logs_one_error_when_identity_cannot_be_remembered`, preserve `#[tokio::test(flavor = "current_thread")]`, replace its process-global/local collector setup with:

```rust
let capture = crate::test_support::TracingCapture::new();
let _tracing_guard = capture.install();
```

Replace formatted/local collector assertions with:

```rust
let errors = capture.matching(
    "last_connected_lv1_save_failed",
    tracing::Level::ERROR,
);
assert_eq!(errors.len(), 1);
assert_eq!(
    errors[0].message.as_deref(),
    Some("Connected to LV1, but the connection could not be remembered for next startup")
);
```

Remove imports and local capture code only when no other lifecycle test still uses them. Do not migrate the unrelated formatter-byte tests in `src-tauri/src/logging.rs`.

- [ ] **Step 6: Run focused and crate-wide Rust verification**

Run:

```bash
cargo nextest run -p advanced-show-control test_support::tests
cargo nextest run -p advanced-show-control lifecycle::tests::accepted_connect_logs_one_error_when_identity_cannot_be_remembered
cargo nextest run -p advanced-show-control
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
```

Expected: all tests and checks pass with no production logging changes.

- [ ] **Step 7: Run the issue smoke checkpoint**

Run:

```bash
make smoke
```

Read `logs/debug-smoke-report.txt`. Expected: the authoritative suite result reports success. If it fails, fix the regression, rerun affected Rust checks, rerun `make smoke`, and reread the report.

- [ ] **Step 8: Commit the verified issue**

```bash
git status --short
git diff -- src-tauri/src/lib.rs src-tauri/src/test_support.rs src-tauri/src/lifecycle/mod.rs
git add src-tauri/src/lib.rs src-tauri/src/test_support.rs src-tauri/src/lifecycle/mod.rs
git commit -m "test: add shared tracing capture"
```

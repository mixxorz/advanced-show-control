use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tokio::sync::watch;
use tracing::field::{Field, Visit};
use tracing::{Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::{LookupSpan, Registry};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CapturedTracingEvent {
    pub(crate) level: Level,
    pub(crate) event: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) fields: BTreeMap<String, String>,
}

#[derive(Clone)]
pub(crate) struct TracingCapture {
    events: Arc<Mutex<Vec<CapturedTracingEvent>>>,
    revision: watch::Sender<u64>,
}

impl Default for TracingCapture {
    fn default() -> Self {
        let (revision, _) = watch::channel(0);
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            revision,
        }
    }
}

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
        self.events
            .lock()
            .expect("tracing capture lock poisoned")
            .clone()
    }

    pub(crate) fn matching(&self, event: &str, level: Level) -> Vec<CapturedTracingEvent> {
        self.events()
            .into_iter()
            .filter(|captured| captured.level == level && captured.event.as_deref() == Some(event))
            .collect()
    }

    pub(crate) async fn wait_for_matching(
        &self,
        event: &str,
        level: Level,
        predicate: impl Fn(&CapturedTracingEvent) -> bool,
    ) -> CapturedTracingEvent {
        let mut revisions = self.revision.subscribe();
        loop {
            if let Some(captured) = self.matching(event, level).into_iter().find(&predicate) {
                return captured;
            }
            revisions
                .changed()
                .await
                .expect("tracing capture revision channel closed");
        }
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
        self.revision
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }
}

#[derive(Default)]
struct FieldVisitor {
    fields: BTreeMap<String, String>,
}

impl Visit for FieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_string(), format!("{value:?}"));
    }
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
        assert_eq!(
            events[0].fields.get("scene").map(String::as_str),
            Some("4: Chorus")
        );
        assert_eq!(
            events[0].fields.get("attempt").map(String::as_str),
            Some("3")
        );
        assert_eq!(
            events[0].fields.get("lockout_enabled").map(String::as_str),
            Some("true")
        );
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
        assert!(
            first
                .events()
                .iter()
                .all(|event| event.event.as_deref() != Some("second"))
        );
        assert_eq!(second.matching("second", Level::WARN).len(), 1);
        assert!(
            second
                .events()
                .iter()
                .all(|event| event.event.as_deref() != Some("first"))
        );
    }

    #[tokio::test]
    async fn waits_for_the_next_event_matching_structured_fields() {
        let capture = TracingCapture::new();
        let _guard = capture.install();

        let wait = capture.wait_for_matching("scene_recall_skipped", Level::DEBUG, |event| {
            event.fields.get("reason").map(String::as_str) == Some("baseline")
        });
        let emit = async {
            tracing::debug!(
                event = "scene_recall_skipped",
                reason = "different event",
                "Different event"
            );
            tokio::task::yield_now().await;
            tracing::debug!(
                event = "scene_recall_skipped",
                reason = "baseline",
                "Baseline observed"
            );
        };

        let (event, ()) = tokio::join!(wait, emit);
        assert_eq!(event.message.as_deref(), Some("Baseline observed"));
    }
}

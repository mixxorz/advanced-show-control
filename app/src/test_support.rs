use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

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

#[derive(Clone, Default)]
pub(crate) struct TracingCapture {
    events: Arc<Mutex<Vec<CapturedTracingEvent>>>,
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
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_string(), format!("{value:?}"));
    }
}

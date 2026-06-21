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
    pub fn snapshot(&self) -> Vec<SmokeTraceEvent> {
        let inner = self
            .capture
            .inner
            .lock()
            .expect("trace capture lock poisoned");
        inner.events.iter().cloned().collect()
    }

    pub fn finish(self) -> Vec<SmokeTraceEvent> {
        let mut inner = self
            .capture
            .inner
            .lock()
            .expect("trace capture lock poisoned");
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

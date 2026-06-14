use std::error::Error;
use std::fs;
use tauri::{AppHandle, Runtime};
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::{self, FmtContext};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;

use crate::app_state::{LogSeverity, ShellState};
use crate::diagnostics::diagnostic_log_path;

const UI_SINK_TARGET: &str = "advanced_show_control_tauri::logging::ui_sink";

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[cfg_attr(not(test), allow(dead_code))]
pub fn is_missing_event_field(target: &str, field_name: &str) -> bool {
    target.starts_with("advanced_show_control_tauri::")
        && target != UI_SINK_TARGET
        && field_name == "event"
}

pub fn init_logging<R: Runtime>(
    app: &AppHandle<R>,
    state: ShellState,
) -> Result<WorkerGuard, Box<dyn Error>> {
    let log_path = diagnostic_log_path(app);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let (non_blocking, guard) = tracing_appender::non_blocking(file);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let file_layer = fmt::layer()
        .json()
        .with_writer(non_blocking)
        .with_filter(LevelFilter::DEBUG);

    let stdout_layer = fmt::layer()
        .with_target(false)
        .with_ansi(true)
        .event_format(BracketedFormat)
        .with_filter(LevelFilter::DEBUG);

    let ui_layer = UiLogLayer { state }.with_filter(LevelFilter::INFO);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .with(ui_layer)
        .try_init()?;

    Ok(guard)
}

struct BracketedFormat;

impl<S, N> FormatEvent<S, N> for BracketedFormat
where
    S: Subscriber + for<'span> LookupSpan<'span>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        write!(
            writer,
            "[{}] {}",
            event.metadata().level(),
            event.metadata().target()
        )
    }
}

struct UiLogLayer {
    state: ShellState,
}

impl<S> tracing_subscriber::Layer<S> for UiLogLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        if event.metadata().target() == UI_SINK_TARGET {
            return;
        }

        if let Some(severity) = ui_severity(event.metadata().level()) {
            let mut visitor = EventVisitor::default();
            event.record(&mut visitor);
            if let Some(message) = visitor.message {
                let ui_event = UiLogEvent { severity, message };
                let state = self.state.clone();
                tauri::async_runtime::spawn(async move {
                    state.append_log(ui_event.severity, ui_event.message).await;
                });
            }
        }
    }
}

#[derive(Default)]
pub struct EventVisitor {
    pub message: Option<String>,
}

impl tracing::field::Visit for EventVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record_field(field.name(), value);
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "event" || field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        }
    }
}

impl EventVisitor {
    fn record_field(&mut self, field_name: &str, value: &str) {
        if field_name == "event" || field_name == "message" {
            self.message = Some(value.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_severity_drops_debug() {
        assert_eq!(ui_severity(&Level::DEBUG), None);
        assert_eq!(ui_severity(&Level::TRACE), None);
    }

    #[test]
    fn ui_severity_maps_info_warn_error() {
        assert_eq!(ui_severity(&Level::INFO), Some(LogSeverity::Info));
        assert_eq!(ui_severity(&Level::WARN), Some(LogSeverity::Warning));
        assert_eq!(ui_severity(&Level::ERROR), Some(LogSeverity::Error));
    }

    #[test]
    fn event_requires_event_field_for_application_logs() {
        assert!(is_missing_event_field(
            "advanced_show_control_tauri::app_log",
            "event"
        ));
        assert!(!is_missing_event_field(
            "advanced_show_control_tauri::app_log",
            "message"
        ));
        assert!(!is_missing_event_field("other", "event"));
    }

    #[test]
    fn event_visitor_preserves_quoted_messages() {
        let mut visitor = EventVisitor::default();
        visitor.record_field("message", "Starting \"Advanced Show Control\"");
        assert_eq!(
            visitor.message.as_deref(),
            Some("Starting \"Advanced Show Control\"")
        );
    }
}

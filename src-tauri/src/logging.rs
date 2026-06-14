use std::error::Error;
use std::fs;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::mpsc;
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

fn default_env_filter() -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::new(default_env_filter_directive())
}

fn default_env_filter_directive() -> &'static str {
    "debug"
}

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
pub fn is_missing_event_field(fields: &[(&str, &str)]) -> bool {
    let event = fields.iter().find(|(name, _)| *name == "event");
    match event {
        Some((_, value)) => value.is_empty(),
        None => true,
    }
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
        .unwrap_or_else(|_| default_env_filter());

    let file_layer = fmt::layer()
        .json()
        .with_writer(non_blocking)
        .with_filter(LevelFilter::DEBUG);

    let stdout_layer = fmt::layer()
        .with_target(false)
        .with_ansi(true)
        .event_format(BracketedFormat)
        .with_filter(LevelFilter::DEBUG);

    let (ui_tx, ui_rx) = mpsc::channel(64);
    let ui_layer = UiLogLayer { tx: ui_tx }.with_filter(LevelFilter::INFO);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .with(ui_layer)
        .try_init()?;

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        ui_log_projector(app, state, ui_rx).await;
    });

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
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        write!(
            writer,
            "[{}] [{}] [{}] ",
            crate::time::current_timestamp_millis(),
            event.metadata().level(),
            event.metadata().target()
        )
        .and_then(|_| ctx.field_format().format_fields(writer.by_ref(), event))
    }
}

struct UiLogLayer {
    tx: mpsc::Sender<UiLogEvent>,
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
                match self.tx.try_send(ui_event) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(ui_event)) => {
                        tracing::warn!(
                            target: UI_SINK_TARGET,
                            event = "ui_log_channel_full",
                            severity = ?ui_event.severity,
                            "UI log channel full; dropping UI log entry"
                        );
                    }
                    Err(mpsc::error::TrySendError::Closed(ui_event)) => {
                        tracing::error!(
                            target: UI_SINK_TARGET,
                            event = "ui_log_channel_closed",
                            severity = ?ui_event.severity,
                            "UI log channel closed; dropping UI log entry"
                        );
                    }
                }
            }
        }
    }
}

async fn ui_log_projector<R: Runtime>(
    app: AppHandle<R>,
    state: ShellState,
    mut rx: mpsc::Receiver<UiLogEvent>,
) {
    while let Some(ui_event) = rx.recv().await {
        state
            .append_log(ui_event.severity.clone(), ui_event.message)
            .await;
        let snapshot = state.snapshot().await;
        if let Err(err) = app.emit("app-status-changed", &snapshot) {
            tracing::debug!(
                target: UI_SINK_TARGET,
                event = "app_status_emit_failed",
                error = %err,
                "failed to emit app-status-changed after UI log append"
            );
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
    use tracing::subscriber::with_default;
    use tracing_subscriber::registry;

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
        assert!(is_missing_event_field(&[]));
        assert!(is_missing_event_field(&[("message", "hello")]));
        assert!(!is_missing_event_field(&[
            ("event", "scene_recall_blocked"),
            ("message", "Scene recall blocked")
        ]));
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

    #[test]
    fn ui_log_channel_error_targets_are_internal() {
        assert!(!is_missing_event_field(&[("event", "ui_log_channel_full")]));
    }

    #[test]
    fn safety_log_messages_are_ui_visible_levels() {
        assert_eq!(ui_severity(&Level::WARN), Some(LogSeverity::Warning));
        assert_eq!(ui_severity(&Level::ERROR), Some(LogSeverity::Error));
    }

    #[test]
    fn safety_events_have_required_event_names() {
        assert!(!is_missing_event_field(&[
            ("event", "scene_recall_blocked"),
            ("message", "Scene recall blocked")
        ]));
        assert!(!is_missing_event_field(&[
            ("event", "fade_aborted"),
            ("message", "Fade aborted")
        ]));
        assert!(!is_missing_event_field(&[
            ("event", "fade_manual_override"),
            ("message", "Fade manual override detected")
        ]));
        assert!(!is_missing_event_field(&[
            ("event", "command_failed"),
            ("message", "Command failed")
        ]));
    }

    #[test]
    fn ui_layer_projects_safety_warn_event() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let subscriber = registry().with(UiLogLayer { tx });

        with_default(subscriber, || {
            tracing::warn!(
                event = "scene_recall_blocked",
                message = "Scene recall blocked"
            );
        });

        let rt = tokio::runtime::Runtime::new().unwrap();
        let event = rt.block_on(async { rx.recv().await.unwrap() });
        assert_eq!(event.severity, LogSeverity::Warning);
        assert_eq!(event.message, "Scene recall blocked");
        assert!(rt.block_on(async { rx.try_recv().is_err() }));
    }

    #[test]
    fn ui_layer_projects_second_safety_warn_event_once() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let subscriber = registry().with(UiLogLayer { tx });

        with_default(subscriber, || {
            tracing::warn!(event = "fade_aborted", message = "Fade aborted");
        });

        let rt = tokio::runtime::Runtime::new().unwrap();
        let event = rt.block_on(async { rx.recv().await.unwrap() });
        assert_eq!(event.severity, LogSeverity::Warning);
        assert_eq!(event.message, "Fade aborted");
        assert!(rt.block_on(async { rx.try_recv().is_err() }));
    }

    #[test]
    fn ui_layer_projects_command_failure_error_event() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let subscriber = registry().with(UiLogLayer { tx });

        with_default(subscriber, || {
            tracing::error!(event = "command_failed", message = "Command failed");
        });

        let rt = tokio::runtime::Runtime::new().unwrap();
        let event = rt.block_on(async { rx.recv().await.unwrap() });
        assert_eq!(event.severity, LogSeverity::Error);
        assert_eq!(event.message, "Command failed");
        assert!(rt.block_on(async { rx.try_recv().is_err() }));
    }

    #[test]
    fn default_env_filter_uses_debug() {
        assert_eq!(default_env_filter_directive(), "debug");
        assert_eq!(default_env_filter().to_string(), "debug");
    }
}

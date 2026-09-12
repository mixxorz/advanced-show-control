use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::broadcast;
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::{self, FmtContext};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;

use crate::diagnostics::diagnostic_log_path;
use crate::projector::LogSeverity;
use crate::runtime::events::{AppEvent, log_lagged_subscriber};
use crate::settings::{AppSettings, SettingsEvent};

const UI_SINK_TARGET: &str = "advanced_show_control::logging::ui_sink";

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

/// @cc [owner:mixxorz,label:observability] frontend-log-level-boundary
/// This function MUST map `ERROR`, `WARN`, and `INFO` to `Error`, `Warning`, and `Info` respectively,
/// and MUST return `None` for `DEBUG` and `TRACE`.
pub fn ui_severity(level: &Level) -> Option<LogSeverity> {
    match *level {
        Level::ERROR => Some(LogSeverity::Error),
        Level::WARN => Some(LogSeverity::Warning),
        Level::INFO => Some(LogSeverity::Info),
        Level::DEBUG | Level::TRACE => None,
    }
}

#[derive(Clone)]
pub struct DiagnosticFileGate {
    extensive_diagnostics_enabled: Arc<AtomicBool>,
}

impl DiagnosticFileGate {
    fn bootstrap_debug() -> Self {
        Self {
            extensive_diagnostics_enabled: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn set_extensive_diagnostics_enabled(&self, enabled: bool) {
        self.extensive_diagnostics_enabled
            .store(enabled, Ordering::Relaxed);
    }

    fn min_level(&self) -> LevelFilter {
        if self.extensive_diagnostics_enabled.load(Ordering::Relaxed) {
            LevelFilter::DEBUG
        } else {
            LevelFilter::INFO
        }
    }
}

impl<S> tracing_subscriber::layer::Filter<S> for DiagnosticFileGate {
    fn enabled(
        &self,
        meta: &tracing::Metadata<'_>,
        ctx: &tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        tracing_subscriber::layer::Filter::enabled(&self.min_level(), meta, ctx)
    }
}

/// @cc [owner:mixxorz,label:reliability] logging-worker-lifetime
/// The returned runtime's worker guard MUST remain owned by application state for the application
/// lifetime so queued diagnostic records continue to flush.
pub struct LoggingRuntime {
    pub guard: WorkerGuard,
    pub ui_logs: broadcast::Sender<UiLogEvent>,
    diagnostic_file_gate: DiagnosticFileGate,
}

impl LoggingRuntime {
    pub fn set_extensive_diagnostics_enabled(&self, enabled: bool) {
        self.diagnostic_file_gate
            .set_extensive_diagnostics_enabled(enabled);
    }

    pub fn apply_settings(&self, settings: &AppSettings) {
        apply_settings_to_diagnostic_file_gate(&self.diagnostic_file_gate, settings);
    }

    pub fn spawn_settings_watcher(&self, mut events: broadcast::Receiver<AppEvent>) {
        let gate = self.diagnostic_file_gate.clone();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(AppEvent::Settings(SettingsEvent::StateChanged { settings })) => {
                        apply_settings_to_diagnostic_file_gate(&gate, &settings);
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        log_lagged_subscriber("logging-settings", count);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            tracing::debug!(
                event = "logging_settings_watcher_stopped",
                "Logging settings watcher stopped"
            );
        });
    }
}

fn apply_settings_to_diagnostic_file_gate(gate: &DiagnosticFileGate, settings: &AppSettings) {
    gate.set_extensive_diagnostics_enabled(settings.enable_extensive_diagnostics);
}

/// @cc [owner:mixxorz,label:observability] logging-sink-delivery
/// Initialization MUST install an append-only JSON diagnostic-file sink beneath the explicitly
/// supplied platform app-config directory, a human-readable stdout sink, and the frontend sink.
/// Subject to the process-wide environment filter, the file and stdout
/// sinks MUST admit `DEBUG` and above at bootstrap, while the frontend sink MUST admit only `INFO`
/// and above; disabling extensive diagnostics MUST raise only the file sink's minimum level to
/// `INFO`.
pub fn init_logging(app_config_dir: &Path) -> Result<LoggingRuntime, Box<dyn Error>> {
    let log_path = diagnostic_log_path(app_config_dir);
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

    let diagnostic_file_gate = DiagnosticFileGate::bootstrap_debug();

    let file_layer = fmt::layer()
        .json()
        .with_writer(non_blocking)
        .with_filter(diagnostic_file_gate.clone());

    let stdout_layer = fmt::layer()
        .with_target(false)
        .with_ansi(true)
        .event_format(BracketedFormat)
        .with_filter(LevelFilter::DEBUG);

    let (ui_tx, _ui_rx) = broadcast::channel(64);
    // UI logs are routed separately from the projector's app-status snapshots.
    let ui_layer = UiLogLayer { tx: ui_tx.clone() }.with_filter(LevelFilter::INFO);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .with(ui_layer)
        .try_init()?;

    Ok(LoggingRuntime {
        guard,
        ui_logs: ui_tx,
        diagnostic_file_gate,
    })
}

struct BracketedFormat;

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_DIM: &str = "\x1b[2m";
const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_RED: &str = "\x1b[31m";
const ANSI_MAGENTA: &str = "\x1b[35m";

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
        let stdout_event = StdoutEvent::from_event(event);
        write!(
            writer,
            "{}{:>5}{} {}│{} {}[{}]{} {}[{}]{} ",
            level_color(event.metadata().level()),
            event.metadata().level(),
            ANSI_RESET,
            ANSI_DIM,
            ANSI_RESET,
            ANSI_DIM,
            stdout_timestamp(),
            ANSI_RESET,
            ANSI_CYAN,
            stdout_category(event.metadata().target()),
            ANSI_RESET,
        )
        .and_then(|_| {
            if let Some(message) = stdout_osc_message(event) {
                write!(writer, "{message}")
            } else if let Some(message) = stdout_event.message.as_deref() {
                write!(writer, "{message}")
            } else {
                ctx.field_format().format_fields(writer.by_ref(), event)
            }
        })
        .and_then(|_| stdout_event.format_fields(&mut writer))
        .and_then(|_| writeln!(writer))
    }
}

fn stdout_timestamp() -> String {
    let millis = crate::time::current_timestamp_millis()
        .parse::<u128>()
        .unwrap_or_default();
    let millis_in_day = millis % 86_400_000;
    let hours = millis_in_day / 3_600_000;
    let minutes = (millis_in_day % 3_600_000) / 60_000;
    let seconds = (millis_in_day % 60_000) / 1_000;
    let millis = millis_in_day % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

fn level_color(level: &Level) -> &'static str {
    match *level {
        Level::ERROR => ANSI_RED,
        Level::WARN => ANSI_YELLOW,
        Level::INFO => ANSI_GREEN,
        Level::DEBUG => ANSI_MAGENTA,
        Level::TRACE => ANSI_DIM,
    }
}

fn stdout_category(target: &str) -> &str {
    target.rsplit("::").next().unwrap_or(target)
}

#[derive(Default)]
struct StdoutEvent {
    message: Option<String>,
    fields: Vec<(String, String)>,
}

impl StdoutEvent {
    fn from_event(event: &Event<'_>) -> Self {
        let mut visitor = Self::default();
        event.record(&mut visitor);
        visitor
    }

    fn format_fields(&self, writer: &mut Writer<'_>) -> std::fmt::Result {
        let mut fields = self.fields.iter().filter(|(name, _)| name != "message");
        if let Some((name, value)) = fields.next() {
            write!(writer, " {}│ ", ANSI_DIM)?;
            write_field(writer, name, value)?;
            for (name, value) in fields {
                write!(writer, " ")?;
                write_field(writer, name, value)?;
            }
            write!(writer, "{ANSI_RESET}")?;
        }
        Ok(())
    }
}

impl tracing::field::Visit for StdoutEvent {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let value = format!("{value:?}");
        if field.name() == "message" {
            self.message = Some(trim_debug_string_quotes(&value));
        }
        self.fields.push((field.name().to_string(), value));
    }
}

fn write_field(writer: &mut Writer<'_>, name: &str, value: &str) -> std::fmt::Result {
    write!(writer, "{name}=\"{}\"", escaped_field_value(value))
}

fn escaped_field_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn trim_debug_string_quotes(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .to_string()
}

fn stdout_osc_message(event: &Event<'_>) -> Option<String> {
    let mut visitor = StdoutOscMessageVisitor::default();
    event.record(&mut visitor);
    if visitor.event_name.as_deref() == Some("osc_message") {
        match (visitor.direction.as_deref(), visitor.message) {
            (Some(direction), Some(message)) => {
                Some(format!("OSC {} {message}", direction.to_uppercase()))
            }
            (_, message) => message,
        }
    } else {
        None
    }
}

#[derive(Default)]
struct StdoutOscMessageVisitor {
    event_name: Option<String>,
    direction: Option<String>,
    message: Option<String>,
}

impl tracing::field::Visit for StdoutOscMessageVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record_field(field.name(), value.to_string());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "event" || field.name() == "message" {
            self.record_field(
                field.name(),
                trim_debug_string_quotes(&format!("{value:?}")),
            );
        }
    }
}

impl StdoutOscMessageVisitor {
    fn record_field(&mut self, field_name: &str, value: String) {
        match field_name {
            "event" => self.event_name = Some(value),
            "direction" => self.direction = Some(value),
            "message" => self.message = Some(value),
            _ => {}
        }
    }
}

/// @cc [owner:mixxorz,label:privacy;observability] frontend-log-payload
/// A frontend log event MUST contain only its mapped severity and the tracing `message`, falling
/// back to the stable `event` field when no message exists. Other structured fields MUST NOT cross
/// this sink, and events emitted by this sink's own target MUST NOT be re-enqueued.
struct UiLogLayer {
    tx: broadcast::Sender<UiLogEvent>,
}

impl<S> tracing_subscriber::Layer<S> for UiLogLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        // Internal sink errors must not be re-enqueued into the same UI log channel.
        if event.metadata().target() == UI_SINK_TARGET {
            return;
        }

        if let Some(severity) = ui_severity(event.metadata().level()) {
            let mut visitor = EventVisitor::default();
            event.record(&mut visitor);
            if let Some(message) = visitor.ui_message() {
                let ui_event = UiLogEvent { severity, message };
                // UI logs are best-effort; there may be no active projector yet.
                let _ = self.tx.send(ui_event);
            }
        }
    }
}

#[derive(Default)]
pub struct EventVisitor {
    pub event_name: Option<String>,
    pub message: Option<String>,
}

impl tracing::field::Visit for EventVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record_field(field.name(), value);
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "event" || field.name() == "message" {
            self.record_field(field.name(), format!("{value:?}"));
        }
    }
}

impl EventVisitor {
    fn record_field(&mut self, field_name: &str, value: impl Into<String>) {
        match field_name {
            "event" => self.event_name = Some(value.into()),
            "message" => self.message = Some(value.into()),
            _ => {}
        }
    }

    fn ui_message(self) -> Option<String> {
        self.message.or(self.event_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing::subscriber::with_default;
    use tracing_subscriber::registry;

    struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for CapturedWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn diagnostic_file_gate_tracks_extensive_diagnostics_setting() {
        let gate = DiagnosticFileGate::bootstrap_debug();
        let disabled = crate::settings::AppSettings {
            enable_extensive_diagnostics: false,
            ..Default::default()
        };
        let enabled = crate::settings::AppSettings {
            enable_extensive_diagnostics: true,
            ..Default::default()
        };

        assert_eq!(gate.min_level(), LevelFilter::DEBUG);
        apply_settings_to_diagnostic_file_gate(&gate, &disabled);
        assert_eq!(gate.min_level(), LevelFilter::INFO);
        apply_settings_to_diagnostic_file_gate(&gate, &enabled);
        assert_eq!(gate.min_level(), LevelFilter::DEBUG);
    }

    #[test]
    fn stdout_formatter_renders_severity_stripe_lines() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let writer_capture = captured.clone();
        let subscriber = registry().with(
            fmt::layer()
                .with_target(false)
                .event_format(BracketedFormat)
                .with_writer(move || CapturedWriter(writer_capture.clone())),
        );

        with_default(subscriber, || {
            tracing::info!(
                event = "app_started",
                version = "0.1.0",
                "Starting \"Advanced Show Control\""
            );
            tracing::warn!(
                event = "scene_recall_blocked",
                scene = "4: Chorus",
                reason = "lockout enabled",
                "Scene recall blocked for 4: Chorus: lockout enabled"
            );
            tracing::debug!(
                event = "osc_message",
                direction = "rx",
                osc_address = "/CurrentScene",
                "/CurrentScene"
            );
        });

        let output = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
        assert_eq!(
            normalize_stdout_output(&output),
            concat!(
                " INFO │ [TIME] [tests] Starting \"Advanced Show Control\" │ event=\"app_started\" version=\"0.1.0\"\n",
                " WARN │ [TIME] [tests] Scene recall blocked for 4: Chorus: lockout enabled │ event=\"scene_recall_blocked\" scene=\"4: Chorus\" reason=\"lockout enabled\"\n",
                "DEBUG │ [TIME] [tests] OSC RX /CurrentScene │ event=\"osc_message\" direction=\"rx\" osc_address=\"/CurrentScene\"\n",
            )
        );
    }

    fn normalize_stdout_output(output: &str) -> String {
        let stripped = strip_ansi(output);
        stripped
            .lines()
            .map(|line| {
                if let Some(timestamp_start) = line.find('[') {
                    let timestamp_end = timestamp_start + "[00:00:00.000]".len();
                    if line.get(timestamp_start..timestamp_end).is_some() {
                        return format!(
                            "{}[TIME]{}",
                            &line[..timestamp_start],
                            &line[timestamp_end..]
                        );
                    }
                    line.to_string()
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + if stripped.ends_with('\n') { "\n" } else { "" }
    }

    fn strip_ansi(output: &str) -> String {
        let mut stripped = String::new();
        let mut chars = output.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                chars.next();
                for ch in chars.by_ref() {
                    if ch.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                stripped.push(ch);
            }
        }
        stripped
    }

    #[test]
    fn ui_layer_projects_messages_and_filters_diagnostic_and_internal_events() {
        let (tx, mut rx) = broadcast::channel(8);
        let subscriber = registry().with(UiLogLayer { tx });

        with_default(subscriber, || {
            tracing::trace!(event = "transport_poll", "Transport polling");
            tracing::debug!(event = "scene_alignment", "Scene alignment details");
            tracing::error!(target: UI_SINK_TARGET, event = "ui_sink_failed", "Internal sink error");
            tracing::info!(event = "session_saved", "Session saved");
            tracing::warn!(event = "scene_recall_blocked", "Scene recall blocked");
            tracing::error!(event = "command_failed", "Command failed");
            tracing::info!(event = "message_missing");
        });

        for (severity, message) in [
            (LogSeverity::Info, "Session saved"),
            (LogSeverity::Warning, "Scene recall blocked"),
            (LogSeverity::Error, "Command failed"),
            (LogSeverity::Info, "message_missing"),
        ] {
            let event = rx.try_recv().unwrap();
            assert_eq!(event.severity, severity);
            assert_eq!(event.message, message);
        }
        assert!(rx.try_recv().is_err());
    }
}

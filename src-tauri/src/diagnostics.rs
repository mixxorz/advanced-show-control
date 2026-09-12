use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, Runtime};

/// @cc [owner:mixxorz,label:observability;platform] diagnostic-file-location-and-accumulation
/// Each process run MUST target a process-specific `logs/diagnostics-<epoch-ms>-<pid>.jsonl` beneath
/// the platform app-config directory, falling back to the system temporary directory when that
/// directory is unavailable. Path selection MUST leave prior logs untouched, so diagnostic files
/// accumulate across runs unless cleanup is performed outside this function.
pub fn diagnostic_log_path<R: Runtime>(app: &AppHandle<R>) -> std::path::PathBuf {
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

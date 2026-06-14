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

#[allow(dead_code)]
#[derive(Clone)]
pub struct DiagnosticLogPath(pub PathBuf);

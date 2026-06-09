use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, Runtime};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticEntry<'a> {
    timestamp_ms: u128,
    source: &'a str,
    message: &'a str,
}

pub fn diagnostic_log_path<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("diagnostics.jsonl")
}

pub fn append_diagnostic(path: &Path, source: &str, message: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create diagnostics folder: {err}"))?;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("failed to open diagnostics log: {err}"))?;
    let entry = DiagnosticEntry {
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        source,
        message,
    };
    serde_json::to_writer(&mut file, &entry)
        .map_err(|err| format!("failed to write diagnostics entry: {err}"))?;
    file.write_all(b"\n")
        .map_err(|err| format!("failed to finish diagnostics entry: {err}"))
}

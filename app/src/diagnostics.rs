use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// @cc [owner:mixxorz,label:observability;platform] diagnostic-file-location-and-accumulation
/// Each process run MUST target a process-specific `logs/diagnostics-<epoch-ms>-<pid>.jsonl` beneath
/// the explicitly supplied platform app-config directory. Path selection MUST leave prior logs
/// untouched, so diagnostic files accumulate across runs unless cleanup is performed outside this
/// function.
pub fn diagnostic_log_path(app_config_dir: &Path) -> PathBuf {
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    app_config_dir.join("logs").join(format!(
        "diagnostics-{started_at}-{}.jsonl",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::diagnostic_log_path;

    #[test]
    fn diagnostic_log_path_is_process_specific_beneath_explicit_app_config_dir() {
        let app_config_dir = std::path::Path::new("/platform/app-config");

        let path = diagnostic_log_path(app_config_dir);

        assert_eq!(path.parent(), Some(app_config_dir.join("logs").as_path()));
        let file_name = path.file_name().unwrap().to_string_lossy();
        assert!(file_name.starts_with("diagnostics-"));
        assert!(file_name.ends_with(&format!("-{}.jsonl", std::process::id())));
    }
}

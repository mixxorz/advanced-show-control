use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const REPORT_RELATIVE_PATH: &str = "logs/debug-smoke-report.txt";

pub struct SmokeReport {
    path: PathBuf,
    file: File,
}

impl SmokeReport {
    /// Creates and truncates the authoritative report before any runtime setup is attempted.
    pub fn create(repo_root: &Path) -> io::Result<Self> {
        Self::create_at(repo_root.join(REPORT_RELATIVE_PATH))
    }

    fn create_at(path: PathBuf) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)?;
        file.write_all(b"LV1 hardware smoke report\n\n")?;
        file.flush()?;
        Ok(Self { path, file })
    }

    pub fn line(&mut self, line: impl AsRef<str>) -> io::Result<()> {
        writeln!(self.file, "{}", line.as_ref())?;
        self.file.flush()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug, Default)]
pub struct SuiteResult {
    first_failure: Option<String>,
}

impl SuiteResult {
    pub fn fail(&mut self, error: impl std::fmt::Display) {
        if self.first_failure.is_none() {
            self.first_failure = Some(error.to_string());
        }
    }

    pub fn is_ok(&self) -> bool {
        self.first_failure.is_none()
    }

    pub fn first_failure(&self) -> Option<&str> {
        self.first_failure.as_deref()
    }
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("dev-tools must remain below the repository root")
        .to_path_buf()
}

pub async fn wait_until<T>(
    timeout: Duration,
    label: &str,
    mut check: impl AsyncFnMut() -> Option<T>,
) -> Result<T, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = check().await {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for {label}"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("asc-smoke-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn report_is_truncated_and_each_line_is_immediately_visible() {
        let root = temp_path("report");
        let path = root.join(REPORT_RELATIVE_PATH);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "stale SUITE PASS\n").unwrap();

        let mut report = SmokeReport::create(&root).unwrap();
        report.line("START").unwrap();
        report.line("SUITE FAIL").unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "LV1 hardware smoke report\n\nSTART\nSUITE FAIL\n"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn report_creation_failure_does_not_preserve_a_stale_result() {
        let root = temp_path("report-failure");
        let path = root.join(REPORT_RELATIVE_PATH);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("stale"), "SUITE PASS\n").unwrap();

        assert!(SmokeReport::create(&root).is_err());
        assert_eq!(
            std::fs::read_to_string(path.join("stale")).unwrap(),
            "SUITE PASS\n"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn suite_result_keeps_the_first_failure() {
        let mut result = SuiteResult::default();
        result.fail("suite failed");
        result.fail("cleanup failed");

        assert!(!result.is_ok());
        assert_eq!(result.first_failure(), Some("suite failed"));
    }

    #[tokio::test]
    async fn wait_until_returns_values_and_names_timeouts() {
        let mut calls = 0;
        let value = wait_until(Duration::from_secs(1), "value", async || {
            calls += 1;
            (calls == 2).then_some(42)
        })
        .await
        .unwrap();
        assert_eq!(value, 42);

        let error = wait_until(Duration::ZERO, "missing state", async || None::<()>)
            .await
            .unwrap_err();
        assert_eq!(error, "timed out waiting for missing state");
    }
}

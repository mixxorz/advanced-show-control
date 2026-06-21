use crate::connection_state::Lv1SystemIdentity;
use crate::lifecycle::AppLifecycle;
use crate::lv1::Lv1Command;
use crate::smoke::{SmokeBackendResult, SmokeTestParams, SmokeTraceCapture};
use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Runtime, State};
use tokio::sync::oneshot;

pub struct SmokeReport {
    path: std::path::PathBuf,
    lock: Mutex<()>,
}

impl SmokeReport {
    pub fn new() -> Self {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
            .join("logs")
            .join("debug-smoke-report.txt");
        let report = Self {
            path,
            lock: Mutex::new(()),
        };
        let _ = report.reset();
        tracing::info!(
            event = "debug_smoke_report_path",
            path = %report.path.display(),
            "Debug smoke report: {}",
            report.path.display()
        );
        report
    }

    fn reset(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(&self.path, "LV1 debug smoke report\n\n").map_err(|error| error.to_string())
    }

    fn append(&self, body: &str) -> Result<(), String> {
        let _guard = self.lock.lock().map_err(|error| error.to_string())?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| error.to_string())?;
        file.write_all(body.as_bytes())
            .map_err(|error| error.to_string())
    }
}

fn log_smoke_result(report: &SmokeReport, result: &SmokeBackendResult) {
    let failed_steps: Vec<&str> = result
        .steps
        .iter()
        .filter(|step| !step.ok)
        .map(|step| step.step.as_str())
        .collect();

    tracing::info!(
        event = "debug_smoke_result",
        test_id = %result.test_id,
        ok = result.ok,
        failed_steps = ?failed_steps,
        "Debug smoke test {}: {}",
        result.test_id,
        if result.ok { "PASS" } else { "FAIL" },
    );

    for step in &result.steps {
        if step.ok {
            tracing::info!(
                event = "debug_smoke_step",
                test_id = %result.test_id,
                step = %step.step,
                ok = true,
                message = %step.message,
                "  PASS {} - {}",
                step.step,
                step.message,
            );
        } else {
            tracing::error!(
                event = "debug_smoke_step",
                test_id = %result.test_id,
                step = %step.step,
                ok = false,
                message = %step.message,
                observed = ?step.observed,
                "  FAIL {} - {}",
                step.step,
                step.message,
            );
        }
    }

    let mut body = format!(
        "TEST {} {}\nstarted: {}\nfinished: {}\n",
        result.test_id,
        if result.ok { "PASS" } else { "FAIL" },
        result.started_at,
        result.finished_at
    );
    if !failed_steps.is_empty() {
        body.push_str(&format!("failed_steps: {failed_steps:?}\n"));
    }
    for step in &result.steps {
        body.push_str(&format!(
            "STEP {} {} - {}\n",
            step.step,
            if step.ok { "PASS" } else { "FAIL" },
            step.message
        ));
        if !step.ok
            && let Some(observed) = &step.observed
        {
            body.push_str(&format!("OBSERVED {}: {observed}\n", step.step));
        }
    }
    body.push('\n');
    if let Err(error) = report.append(&body) {
        tracing::error!(event = "debug_smoke_report_write_failed", %error, "Failed to write debug smoke report");
    }
}

fn log_smoke_error(report: &SmokeReport, test_id: &str, error: &str) {
    tracing::error!(
        event = "debug_smoke_result",
        test_id,
        ok = false,
        message = error,
        "Debug smoke test {test_id}: ERROR - {error}",
    );
    if let Err(write_error) = report.append(&format!("TEST {test_id} ERROR\nerror: {error}\n\n")) {
        tracing::error!(event = "debug_smoke_report_write_failed", error = %write_error, "Failed to write debug smoke report");
    }
}

async fn run_and_log(
    report: &SmokeReport,
    test_id: &'static str,
    run: impl std::future::Future<Output = Result<SmokeBackendResult, String>>,
) -> Result<SmokeBackendResult, String> {
    match run.await {
        Ok(result) => {
            log_smoke_result(report, &result);
            Ok(result)
        }
        Err(error) => {
            log_smoke_error(report, test_id, &error);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn debug_smoke_finish_suite(
    report: State<'_, SmokeReport>,
    ok: bool,
    message: Option<String>,
) -> Result<(), String> {
    let status = if ok { "PASS" } else { "FAIL" };
    let message = message.unwrap_or_default();
    report.append(&format!("SUITE {status}\n{message}\n"))?;
    tracing::info!(event = "debug_smoke_suite_result", ok, message = %message, "Debug smoke suite: {status}");
    Ok(())
}

#[tauri::command]
pub async fn debug_smoke_exit_app<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        app.exit(0);
    });
    Ok(())
}

#[tauri::command]
pub async fn debug_smoke_report_setup(
    lifecycle: State<'_, AppLifecycle>,
    report: State<'_, SmokeReport>,
    params: SmokeTestParams,
) -> Result<(), String> {
    let show = lifecycle.current_show().await;
    let mut body = String::from("SETUP\n");
    for scene_id in [&params.scene_a_id, &params.scene_b_id] {
        let (reply, rx) = oneshot::channel();
        show.send(crate::show::ShowCommand::GetSceneConfig {
            scene_id: scene_id.clone(),
            reply,
        })
        .await
        .map_err(|error| error.to_string())?;
        let config = rx
            .await
            .map_err(|_| "show config reply channel closed".to_string())?;
        match config {
            Some(config) => {
                let target = config
                    .channel_configs
                    .iter()
                    .find(|entry| {
                        entry.group == params.channel.group
                            && entry.channel == params.channel.channel
                    })
                    .and_then(|entry| entry.fader_db);
                body.push_str(&format!(
                    "SCENE {scene_id} duration={} target_channel={}:{} target_db={target:?}\n",
                    config.duration_ms, params.channel.group, params.channel.channel
                ));
            }
            None => body.push_str(&format!("SCENE {scene_id} missing\n")),
        }
    }

    if let Some(lv1) = lifecycle.debug_smoke_current_lv1().await {
        let (reply, rx) = oneshot::channel();
        lv1.send(crate::lv1::Lv1Command::GetState { reply })
            .await
            .map_err(|error| error.to_string())?;
        let snapshot = rx
            .await
            .map_err(|_| "LV1 state reply channel closed".to_string())?;
        let live_db = snapshot
            .channels
            .iter()
            .find(|entry| {
                entry.group == params.channel.group && entry.channel == params.channel.channel
            })
            .map(|entry| entry.gain_db);
        body.push_str(&format!(
            "LIVE channel={}:{} db={live_db:?}\n",
            params.channel.group, params.channel.channel
        ));
    } else {
        body.push_str("LIVE unavailable\n");
    }
    body.push('\n');
    report.append(&body)
}

#[tauri::command]
pub async fn debug_smoke_set_channel_gain(
    lifecycle: State<'_, AppLifecycle>,
    params: SmokeTestParams,
    gain_db: f64,
) -> Result<(), String> {
    let lv1 = lifecycle
        .debug_smoke_current_lv1()
        .await
        .ok_or_else(|| "LV1 is unavailable".to_string())?;
    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::SetGain {
        group: params.channel.group,
        channel: params.channel.channel,
        gain_db,
        reply: Some(reply),
    })
    .await
    .map_err(|error| error.to_string())?;
    rx.await
        .map_err(|_| "LV1 gain write reply channel closed".to_string())?
        .map_err(|error| error.to_string())?;

    let deadline = tokio::time::Instant::now() + Duration::from_millis(params.timeout_ms);
    loop {
        let (reply, rx) = oneshot::channel();
        lv1.send(Lv1Command::GetState { reply })
            .await
            .map_err(|error| error.to_string())?;
        let snapshot = rx
            .await
            .map_err(|_| "LV1 state reply channel closed".to_string())?;
        if snapshot
            .channels
            .iter()
            .find(|entry| {
                entry.group == params.channel.group && entry.channel == params.channel.channel
            })
            .is_some_and(|entry| (entry.gain_db - gain_db).abs() <= params.tolerance_db)
        {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for channel {}:{} to reach {gain_db} dB",
                params.channel.group, params.channel.channel
            ));
        }
        tokio::time::sleep(Duration::from_millis(params.sample_interval_ms)).await;
    }
}

#[tauri::command]
pub async fn debug_smoke_run_connection_test<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, AppLifecycle>,
    trace_capture: State<'_, SmokeTraceCapture>,
    report: State<'_, SmokeReport>,
    identity: Lv1SystemIdentity,
    timeout_ms: u64,
) -> Result<SmokeBackendResult, String> {
    run_and_log(
        &report,
        "connection",
        crate::smoke::tests::run_connection_test(
            app,
            &lifecycle,
            identity,
            timeout_ms,
            &trace_capture,
        ),
    )
    .await
}

#[tauri::command]
pub async fn debug_smoke_run_scene_recall_test(
    lifecycle: State<'_, AppLifecycle>,
    trace_capture: State<'_, SmokeTraceCapture>,
    report: State<'_, SmokeReport>,
    params: SmokeTestParams,
    target_scene_id: String,
) -> Result<SmokeBackendResult, String> {
    run_and_log(
        &report,
        "scene-recall",
        crate::smoke::tests::run_scene_recall_test(
            &lifecycle,
            params,
            target_scene_id,
            &trace_capture,
        ),
    )
    .await
}

#[tauri::command]
pub async fn debug_smoke_run_fade_starts_test(
    lifecycle: State<'_, AppLifecycle>,
    trace_capture: State<'_, SmokeTraceCapture>,
    report: State<'_, SmokeReport>,
    params: SmokeTestParams,
) -> Result<SmokeBackendResult, String> {
    run_and_log(
        &report,
        "fade-starts",
        crate::smoke::tests::run_fade_starts_test(&lifecycle, params, &trace_capture),
    )
    .await
}

#[tauri::command]
pub async fn debug_smoke_run_fade_completes_test(
    lifecycle: State<'_, AppLifecycle>,
    trace_capture: State<'_, SmokeTraceCapture>,
    report: State<'_, SmokeReport>,
    params: SmokeTestParams,
    expected_target_db: f64,
) -> Result<SmokeBackendResult, String> {
    run_and_log(
        &report,
        "fade-completes",
        crate::smoke::tests::run_fade_completes_test(
            &lifecycle,
            params,
            expected_target_db,
            &trace_capture,
        ),
    )
    .await
}

#[tauri::command]
pub async fn debug_smoke_run_decreasing_xfade_test(
    lifecycle: State<'_, AppLifecycle>,
    trace_capture: State<'_, SmokeTraceCapture>,
    report: State<'_, SmokeReport>,
    params: SmokeTestParams,
) -> Result<SmokeBackendResult, String> {
    run_and_log(
        &report,
        "decreasing-xfade",
        crate::smoke::tests::run_decreasing_xfade_test(&lifecycle, params, &trace_capture),
    )
    .await
}

#[tauri::command]
pub async fn debug_smoke_run_lockout_blocks_recall_test(
    lifecycle: State<'_, AppLifecycle>,
    trace_capture: State<'_, SmokeTraceCapture>,
    report: State<'_, SmokeReport>,
    params: SmokeTestParams,
) -> Result<SmokeBackendResult, String> {
    run_and_log(
        &report,
        "lockout-blocks-recall",
        crate::smoke::tests::run_lockout_blocks_recall_test(&lifecycle, params, &trace_capture),
    )
    .await
}

use crate::lifecycle::AppLifecycle;
use crate::lv1::Lv1Command;
use crate::show::{
    SHOW_FILE_SCHEMA_VERSION, ShowCommand, ShowFile, ShowFileSafety, ShowFileSceneConfig,
    ShowFileSceneScopeToggles,
};
use crate::show_file::write_show_file;
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
            .join("logs/debug-smoke-report.txt");
        let report = Self {
            path,
            lock: Mutex::new(()),
        };
        if let Some(parent) = report.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&report.path, "LV1 debug smoke report\n\n");
        report
    }

    fn write(&self, line: &str) -> Result<(), String> {
        let _guard = self.lock.lock().map_err(|error| error.to_string())?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| error.to_string())?;
        file.write_all(line.as_bytes())
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
pub fn debug_smoke_log(report: State<'_, SmokeReport>, line: String) -> Result<(), String> {
    report.write(&format!("{line}\n"))
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
pub async fn debug_smoke_set_channel_gain(
    lifecycle: State<'_, AppLifecycle>,
    group: i32,
    channel: i32,
    gain_db: f64,
) -> Result<(), String> {
    let lv1 = lifecycle
        .current_lv1()
        .await
        .ok_or_else(|| "LV1 is unavailable".to_string())?;
    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::SetGain {
        group,
        channel,
        gain_db,
        reply: Some(reply),
    })
    .await
    .map_err(|error| error.to_string())?;
    rx.await
        .map_err(|_| "LV1 gain write reply channel closed".to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn debug_smoke_recall_lv1_scene(
    lifecycle: State<'_, AppLifecycle>,
    scene_index: i32,
) -> Result<(), String> {
    let lv1 = lifecycle
        .current_lv1()
        .await
        .ok_or_else(|| "LV1 is unavailable".to_string())?;
    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::RecallScene {
        scene_index,
        reply: Some(reply),
    })
    .await
    .map_err(|error| error.to_string())?;
    rx.await
        .map_err(|_| "LV1 scene recall reply channel closed".to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn debug_smoke_get_channel_gain(
    lifecycle: State<'_, AppLifecycle>,
    group: i32,
    channel: i32,
) -> Result<f64, String> {
    let lv1 = lifecycle
        .current_lv1()
        .await
        .ok_or_else(|| "LV1 is unavailable".to_string())?;
    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::GetState { reply })
        .await
        .map_err(|error| error.to_string())?;
    let snapshot = rx
        .await
        .map_err(|_| "LV1 state reply channel closed".to_string())?;
    snapshot
        .channels
        .iter()
        .find(|entry| entry.group == group && entry.channel == channel)
        .map(|entry| entry.gain_db)
        .ok_or_else(|| format!("channel {group}:{channel} unavailable"))
}

#[tauri::command]
pub async fn debug_smoke_load_unlinked_scene_session(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<String, String> {
    let internal_scene_id = uuid::Uuid::new_v4();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
        .join("logs/debug-smoke-unlinked-scene.ascs");
    let backup_dir = path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("backups");
    let file = ShowFile {
        schema_version: SHOW_FILE_SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        saved_at: crate::time::current_timestamp_millis(),
        safety: ShowFileSafety { lockout: false },
        scene_configs: vec![ShowFileSceneConfig {
            internal_scene_id: Some(internal_scene_id),
            scene_index: Some(99),
            scene_name: "Debug Smoke Missing Scene".to_string(),
            duration_ms: 1_000,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: ShowFileSceneScopeToggles::default(),
        }],
        cued_scene_internal_id: None,
    };
    write_show_file(&path, &file, &backup_dir)?;

    let show = lifecycle.current_show().await;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::LoadShowFileFromPath {
        path,
        reply: Some(reply),
    })
    .await
    .map_err(|error| error.to_string())?;
    rx.await
        .map_err(|_| "show file load reply channel closed".to_string())??;
    Ok(internal_scene_id.to_string())
}

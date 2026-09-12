use crate::lifecycle::AppLifecycle;
use crate::lv1::{Lv1Command, Lv1Connection};
use crate::scenes::SceneScopeToggles;
use crate::show::{
    SHOW_FILE_SCHEMA_VERSION, ShowCommand, ShowFile, ShowFileSafety, ShowFileSceneConfig,
    ShowStateHandle,
};
use crate::show_file::write_show_file;
use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime, State};
use tokio::sync::oneshot;

pub struct SmokeReport {
    path: std::path::PathBuf,
    lock: Mutex<()>,
}

impl SmokeReport {
    /// @cc [owner:mixxorz,label:debug] smoke-report-is-authoritative-file
    /// Creating a smoke report MUST reset the fixed debug report file before the suite runs. A
    /// setup-write failure MUST be surfaced through tracing and returned so debug app setup fails
    /// rather than running against a missing or stale authoritative report.
    pub fn new() -> std::io::Result<Self> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
            .join("logs/debug-smoke-report.txt");
        match Self::new_at(path.clone()) {
            Ok(report) => {
                tracing::info!(
                    event = "debug_smoke_report_started",
                    path = %path.display(),
                    "Debug smoke report started"
                );
                Ok(report)
            }
            Err(error) => {
                tracing::error!(
                    event = "debug_smoke_report_start_failed",
                    path = %path.display(),
                    error = %error,
                    "Debug smoke report start failed"
                );
                Err(error)
            }
        }
    }

    fn new_at(path: std::path::PathBuf) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, "LV1 debug smoke report\n\n")?;
        Ok(Self {
            path,
            lock: Mutex::new(()),
        })
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

/// @cc [owner:mixxorz,label:debug;safety] debug-gain-write-awaits-current-lv1
/// This debug-only setup command MUST bind Lifecycle's current LV1 endpoint to its generation,
/// fence mailbox admission and the reply against generation changes, and await the actor's write
/// acknowledgement. Stale generation, unavailability, transport, reply, or rejected-write failures
/// MUST be returned to the smoke runner rather than reported as success.
#[tauri::command]
pub async fn debug_smoke_set_channel_gain(
    lifecycle: State<'_, AppLifecycle>,
    group: i32,
    channel: i32,
    gain_db: f64,
) -> Result<(), String> {
    let lv1 = current_lv1_connection(&lifecycle).await?;
    set_channel_gain(&lv1, group, channel, gain_db).await
}

/// @cc [owner:mixxorz,label:debug;safety] raw-recall-remains-debug-setup-only
/// This debug-only setup command mutates live console state and MUST bind Lifecycle's current LV1
/// endpoint to its generation, fence mailbox admission and the reply against generation changes,
/// and await actor acknowledgement. It MUST remain outside production command registration and
/// bypass Scenes recall policy only to establish deterministic debug smoke preconditions.
#[tauri::command]
pub async fn debug_smoke_recall_lv1_scene(
    lifecycle: State<'_, AppLifecycle>,
    scene_index: i32,
) -> Result<(), String> {
    let lv1 = current_lv1_connection(&lifecycle).await?;
    recall_lv1_scene(&lv1, scene_index).await
}

/// @cc [owner:mixxorz,label:debug] debug-gain-read-uses-actor-snapshot
/// This debug-only observation MUST request a fresh snapshot through a generation-bound connection
/// to Lifecycle's current LV1 actor, fence mailbox admission and the reply against generation
/// changes, and fail when the endpoint, reply, or requested channel is unavailable. It MUST NOT read
/// projected UI state as evidence of the live gain.
#[tauri::command]
pub async fn debug_smoke_get_channel_gain(
    lifecycle: State<'_, AppLifecycle>,
    group: i32,
    channel: i32,
) -> Result<f64, String> {
    let lv1 = current_lv1_connection(&lifecycle).await?;
    read_channel_gain(&lv1, group, channel).await
}

async fn current_lv1_connection(lifecycle: &AppLifecycle) -> Result<Lv1Connection, String> {
    let authority = lifecycle.current_runtime_generation().await;
    let (generation, lv1) = lifecycle
        .runtime_snapshot_source()
        .connected_lv1()
        .await
        .ok_or_else(|| "LV1 is unavailable".to_string())?;
    Ok(Lv1Connection::new(lv1, authority, generation))
}

async fn set_channel_gain(
    lv1: &Lv1Connection,
    group: i32,
    channel: i32,
    gain_db: f64,
) -> Result<(), String> {
    lv1.request(|reply| Lv1Command::SetGain {
        group,
        channel,
        gain_db,
        reply: Some(reply),
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

async fn recall_lv1_scene(lv1: &Lv1Connection, scene_index: i32) -> Result<(), String> {
    lv1.request(|reply| Lv1Command::RecallScene {
        scene_index,
        reply: Some(reply),
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
    .map(|_| ())
}

async fn read_channel_gain(lv1: &Lv1Connection, group: i32, channel: i32) -> Result<f64, String> {
    let snapshot = lv1
        .request(|reply| Lv1Command::GetState { reply })
        .await
        .map_err(|error| error.to_string())?;
    snapshot
        .channels
        .iter()
        .find(|entry| entry.group == group && entry.channel == channel)
        .map(|entry| entry.gain_db)
        .ok_or_else(|| format!("channel {group}:{channel} unavailable"))
}

/// @cc [owner:mixxorz,label:debug] scene-settings-fixture-load-remains-debug-only
/// This command MUST remain debug-only, create only the empty synthetic session used for scene-settings
/// smoke setup, dispatch its path through Show's `LoadShowFileFromPath`, await the owner's result, and
/// propagate fixture-write, mailbox, dropped-reply, and domain failures to the smoke runner.
#[tauri::command]
pub async fn debug_smoke_load_scene_settings_session<R: Runtime>(
    app: AppHandle<R>,
) -> Result<(), String> {
    let path = std::env::temp_dir().join(format!(
        "advanced-show-control-debug-smoke-scene-settings-{}.ascs",
        uuid::Uuid::new_v4()
    ));
    let backup_dir = path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("backups");
    let file = ShowFile {
        schema_version: SHOW_FILE_SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        saved_at: crate::time::current_timestamp_millis(),
        safety: ShowFileSafety { lockout: false },
        scene_configs: Vec::new(),
        cue_lists: Vec::new(),
        active_cue_list_id: None,
        cued_cue_entry_id: None,
    };
    write_show_file(&path, &file, &backup_dir)?;

    let show = app.state::<ShowStateHandle>().inner().clone();
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::LoadShowFileFromPath {
        path,
        reply: Some(reply),
    })
    .await
    .map_err(|error| error.to_string())?;
    rx.await
        .map_err(|_| "show file load reply channel closed".to_string())??;
    Ok(())
}

/// @cc [owner:mixxorz,label:debug] unlinked-scene-fixture-load-remains-debug-only
/// This command MUST remain debug-only, create only the synthetic missing-scene session used for link
/// smoke setup, dispatch its path through Show's `LoadShowFileFromPath`, await the owner's result, and
/// propagate fixture-write, mailbox, dropped-reply, and domain failures to the smoke runner.
#[tauri::command]
pub async fn debug_smoke_load_unlinked_scene_session<R: Runtime>(
    app: AppHandle<R>,
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
            scope_toggles: SceneScopeToggles::default(),
        }],
        cue_lists: Vec::new(),
        active_cue_list_id: None,
        cued_cue_entry_id: None,
    };
    write_show_file(&path, &file, &backup_dir)?;

    let show = app.state::<ShowStateHandle>().inner().clone();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::{Lv1Connection, build_actor};
    use crate::runtime::events::AppEventBus;
    use crate::runtime::generation::RuntimeGeneration;

    #[test]
    fn smoke_report_initialization_failure_cannot_return_a_stale_report() {
        let root = std::env::temp_dir().join(format!(
            "advanced-show-control-smoke-report-test-{}",
            uuid::Uuid::new_v4()
        ));
        let report_path = root.join("debug-smoke-report.txt");
        std::fs::create_dir_all(&report_path).unwrap();
        let stale_marker = report_path.join("stale-result");
        std::fs::write(&stale_marker, "SUITE PASS\n").unwrap();

        let report = SmokeReport::new_at(report_path);

        assert!(report.is_err());
        assert_eq!(
            std::fs::read_to_string(stale_marker).unwrap(),
            "SUITE PASS\n"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn stale_generation_prevents_debug_gain_dispatch() {
        let connection = stale_connection().await;

        let error = set_channel_gain(&connection, 0, 1, -10.0)
            .await
            .unwrap_err();

        assert_eq!(error, "generation is stale");
    }

    #[tokio::test]
    async fn stale_generation_prevents_debug_raw_recall_dispatch() {
        let connection = stale_connection().await;

        let error = recall_lv1_scene(&connection, 1).await.unwrap_err();

        assert_eq!(error, "generation is stale");
    }

    #[tokio::test]
    async fn stale_generation_prevents_debug_gain_read_dispatch() {
        let connection = stale_connection().await;

        let error = read_channel_gain(&connection, 0, 1).await.unwrap_err();

        assert_eq!(error, "generation is stale");
    }

    async fn stale_connection() -> Lv1Connection {
        let authority = RuntimeGeneration::new();
        let generation = authority.current().await;
        let (lv1, _task) = build_actor(
            "127.0.0.1".to_string(),
            9,
            AppEventBus::default(),
            generation,
        );
        let connection = Lv1Connection::new(lv1, authority.clone(), generation);
        authority.advance().await;
        connection
    }
}

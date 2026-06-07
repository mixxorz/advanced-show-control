use std::path::PathBuf;

use super::shell::{ShellInner, ShellState, scene_id, snapshot_from_inner};
use super::view::{AppViewState, FadeTarget, LogSeverity, LogSource, SceneFadeConfig};
use crate::show_file::{
    SHOW_FILE_SCHEMA_VERSION, ShowFile, ShowFileFadeTarget, ShowFileSafety,
    ShowFileSceneFadeConfig, validate_show_file,
};

impl ShellState {
    pub async fn new_show_file(&self) -> Result<AppViewState, String> {
        let mut inner = self.inner.lock().await;

        if inner.listen_mode_active {
            return Err("Stop Listen Mode before creating a new show file".to_string());
        }

        inner.lockout = false;
        inner.scene_fade_configs.clear();
        inner.selected_scene_id = None;
        inner.show_file_path = None;
        inner.show_file_dirty = false;
        inner.show_file_last_saved_at = None;
        inner.unknown_fader_warnings.clear();

        if let Some(scenes) = inner
            .lv1_snapshot
            .as_ref()
            .map(|snapshot| snapshot.scene_list.clone())
        {
            inner.reconcile_scene_fade_configs(&scenes);
        }

        inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            "New show file created".to_string(),
        );
        Ok(snapshot_from_inner(&inner))
    }

    pub async fn export_show_file_for_save(&self, saved_at: String) -> Result<ShowFile, String> {
        let inner = self.inner.lock().await;
        if inner.listen_mode_active {
            return Err("Stop Listen Mode before saving a show file".to_string());
        }

        Ok(show_file_from_inner(&inner, saved_at))
    }

    pub async fn current_show_file_path(&self) -> Option<PathBuf> {
        let inner = self.inner.lock().await;
        inner.show_file_path.clone()
    }

    pub async fn load_show_file_from_dto(
        &self,
        path: PathBuf,
        file: &mut ShowFile,
    ) -> Result<AppViewState, String> {
        let mut inner = self.inner.lock().await;

        if inner.listen_mode_active {
            return Err("Stop Listen Mode before opening a show file".to_string());
        }

        let lv1 = inner.lv1_snapshot.clone().ok_or_else(|| {
            "Open a show file after LV1 scenes and channels are loaded".to_string()
        })?;
        let report = validate_show_file(file, &lv1)?;

        inner.lockout = file.safety.lockout;
        inner.scene_fade_configs = file
            .scene_fade_configs
            .iter()
            .map(scene_config_from_show_file)
            .collect();
        inner.unknown_fader_warnings.clear();
        inner.selected_scene_id = inner
            .scene_fade_configs
            .first()
            .map(|config| config.scene_id.clone());
        inner.show_file_path = Some(path);
        inner.show_file_last_saved_at = Some(file.saved_at.clone());
        inner.show_file_dirty = report.removed_anything();

        for scene in report.removed_scenes {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!("Deleted saved scene config during load: {scene}"),
            );
        }

        for target in report.removed_targets {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!("Deleted saved fader target during load: {target}"),
            );
        }

        inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            "Show file loaded".to_string(),
        );

        Ok(snapshot_from_inner(&inner))
    }

    pub async fn mark_show_file_saved(&self, path: PathBuf, saved_at: String) -> AppViewState {
        let mut inner = self.inner.lock().await;
        inner.show_file_path = Some(path);
        inner.show_file_last_saved_at = Some(saved_at);
        inner.show_file_dirty = false;
        inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            "Show file saved".to_string(),
        );
        snapshot_from_inner(&inner)
    }

    #[cfg(test)]
    pub async fn export_show_file(&self, saved_at: String) -> ShowFile {
        let inner = self.inner.lock().await;
        show_file_from_inner(&inner, saved_at)
    }
}

fn show_file_from_inner(inner: &ShellInner, saved_at: String) -> ShowFile {
    ShowFile {
        schema_version: SHOW_FILE_SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        saved_at,
        safety: ShowFileSafety {
            lockout: inner.lockout,
        },
        scene_fade_configs: inner
            .scene_fade_configs
            .iter()
            .map(|config| ShowFileSceneFadeConfig {
                scene_index: config.scene_index,
                scene_name: config.scene_name.clone(),
                fade_enabled: config.fade_enabled,
                duration_ms: config.duration_ms,
                fade_targets: config
                    .fade_targets
                    .iter()
                    .map(|target| ShowFileFadeTarget {
                        group: target.group,
                        channel: target.channel,
                        channel_name: target.channel_name.clone(),
                        target_db: target.target_db,
                        enabled: target.enabled,
                        updated_at: target.updated_at.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn scene_config_from_show_file(config: &ShowFileSceneFadeConfig) -> SceneFadeConfig {
    SceneFadeConfig {
        scene_id: scene_id(config.scene_index, &config.scene_name),
        scene_index: config.scene_index,
        scene_name: config.scene_name.clone(),
        fade_enabled: config.fade_enabled,
        duration_ms: config.duration_ms,
        fade_targets: config
            .fade_targets
            .iter()
            .map(|target| FadeTarget {
                group: target.group,
                channel: target.channel,
                channel_name: target.channel_name.clone(),
                target_db: target.target_db,
                enabled: target.enabled,
                updated_at: target.updated_at.clone(),
            })
            .collect(),
    }
}

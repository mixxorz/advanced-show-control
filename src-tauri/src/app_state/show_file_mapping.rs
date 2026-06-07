use std::path::PathBuf;

use super::shell::{ShellInner, ShellState, scene_id, snapshot_from_inner};
use super::view::{AppViewState, ChannelConfig, ChannelRef, LogSeverity, LogSource, SceneConfig};
use crate::show_file::{
    SHOW_FILE_SCHEMA_VERSION, ShowFile, ShowFileChannelConfig, ShowFileChannelRef,
    ShowFileSafety, ShowFileSceneConfig, validate_show_file,
};

impl ShellState {
    pub async fn new_show_file(&self) -> Result<AppViewState, String> {
        let mut inner = self.inner.lock().await;

        inner.lockout = false;
        inner.scene_configs.clear();
        inner.selected_scene_id = None;
        inner.show_file_path = None;
        inner.show_file_dirty = false;
        inner.show_file_last_saved_at = None;

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

        let lv1 = inner.lv1_snapshot.clone().ok_or_else(|| {
            "Open a show file after LV1 scenes and channels are loaded".to_string()
        })?;
        let report = validate_show_file(file, &lv1)?;

        inner.lockout = file.safety.lockout;
        inner.scene_configs = file
            .scene_configs
            .iter()
            .map(scene_config_from_show_file)
            .collect();
        inner.selected_scene_id = inner
            .scene_configs
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
        scene_configs: inner
            .scene_configs
            .iter()
            .map(|config| ShowFileSceneConfig {
                scene_index: config.scene_index,
                scene_name: config.scene_name.clone(),
                duration_ms: config.duration_ms,
                channel_configs: config
                    .channel_configs
                    .iter()
                    .map(|target| ShowFileChannelConfig {
                        group: target.group,
                        channel: target.channel,
                        fader_db: target.fader_db,
                    })
                    .collect(),
                scoped_channels: config
                    .scoped_channels
                    .iter()
                    .map(|channel| ShowFileChannelRef {
                        group: channel.group,
                        channel: channel.channel,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn scene_config_from_show_file(config: &ShowFileSceneConfig) -> SceneConfig {
    SceneConfig {
        scene_id: scene_id(config.scene_index, &config.scene_name),
        scene_index: config.scene_index,
        scene_name: config.scene_name.clone(),
        duration_ms: config.duration_ms,
        channel_configs: config
            .channel_configs
            .iter()
            .map(|target| ChannelConfig {
                group: target.group,
                channel: target.channel,
                fader_db: target.fader_db,
            })
            .collect(),
        scoped_channels: config
            .scoped_channels
            .iter()
            .map(|channel| ChannelRef {
                group: channel.group,
                channel: channel.channel,
            })
            .collect(),
    }
}

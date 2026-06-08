use std::path::PathBuf;

use super::shell::ShellState;
use super::view::AppViewState;
use crate::show_file::{
    SHOW_FILE_SCHEMA_VERSION, ShowFile, ShowFileChannelConfig, ShowFileChannelRef, ShowFileSafety,
    ShowFileSceneConfig, ShowFileSceneScopeToggles, validate_show_file,
};

impl ShellState {
    pub async fn new_show_file(&self) -> Result<AppViewState, String> {
        self.show
            .clear()
            .await
            .map_err(|err| format!("Failed to reset show data: {err:?}"))?;

        let scenes = {
            let inner = self.inner.lock().await;
            inner
                .lv1_snapshot
                .as_ref()
                .map(|snapshot| snapshot.scene_list.clone())
                .unwrap_or_default()
        };

        if !scenes.is_empty() {
            self.show
                .reconcile_scene_list(scenes)
                .await
                .map_err(|err| format!("Failed to rebuild show data: {err:?}"))?;
        }

        let selected_scene_id = self
            .show
            .get_snapshot()
            .await
            .ok()
            .and_then(|snapshot| snapshot.scene_configs.first().cloned())
            .map(|scene| scene.scene_id);

        let mut inner = self.inner.lock().await;
        inner.selected_scene_id = selected_scene_id;
        inner.show_file_path = None;
        inner.show_file_dirty = false;
        inner.show_file_last_saved_at = None;
        inner.push_log(
            super::view::LogSource::App,
            super::view::LogSeverity::Info,
            "New show file created".to_string(),
        );
        drop(inner);
        Ok(self.snapshot().await)
    }

    pub async fn export_show_file_for_save(&self, saved_at: String) -> Result<ShowFile, String> {
        let show = self
            .show
            .get_snapshot()
            .await
            .map_err(|err| format!("Failed to read show snapshot: {err:?}"))?;

        Ok(ShowFile {
            schema_version: SHOW_FILE_SCHEMA_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            saved_at,
            safety: ShowFileSafety {
                lockout: show.lockout,
            },
            scene_configs: show
                .scene_configs
                .into_iter()
                .map(|config| ShowFileSceneConfig {
                    scene_index: config.scene_index,
                    scene_name: config.scene_name.clone(),
                    duration_ms: config.duration_ms,
                    channel_configs: config
                        .channel_configs
                        .into_iter()
                        .map(|target| ShowFileChannelConfig {
                            group: target.group,
                            channel: target.channel,
                            fader_db: target.fader_db,
                        })
                        .collect(),
                    scoped_channels: config
                        .scoped_channels
                        .into_iter()
                        .map(|channel| ShowFileChannelRef {
                            group: channel.group,
                            channel: channel.channel,
                        })
                        .collect(),
                    scope_toggles: ShowFileSceneScopeToggles {
                        faders: config.scope_toggles.faders,
                    },
                })
                .collect(),
        })
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
        let inner = self.inner.lock().await;
        let lv1 = inner
            .lv1_snapshot
            .clone()
            .ok_or_else(|| "Open a show file after LV1 scenes are loaded".to_string())?;
        let report = validate_show_file(file, &lv1)?;
        drop(inner);

        self.show
            .replace_snapshot(advanced_show_control::show::types::ShowSnapshot {
                lockout: file.safety.lockout,
                scene_configs: file
                    .scene_configs
                    .iter()
                    .map(|config| advanced_show_control::show::types::SceneConfig {
                        scene_id: format!("{}::{}", config.scene_index, config.scene_name),
                        scene_index: config.scene_index,
                        scene_name: config.scene_name.clone(),
                        duration_ms: config.duration_ms,
                        channel_configs: config
                            .channel_configs
                            .iter()
                            .map(|target| advanced_show_control::show::types::ChannelConfig {
                                group: target.group,
                                channel: target.channel,
                                fader_db: target.fader_db,
                            })
                            .collect(),
                        scoped_channels: config
                            .scoped_channels
                            .iter()
                            .map(|channel| advanced_show_control::show::types::ChannelRef {
                                group: channel.group,
                                channel: channel.channel,
                            })
                            .collect(),
                        scope_toggles: advanced_show_control::show::types::SceneScopeToggles {
                            faders: config.scope_toggles.faders,
                        },
                    })
                    .collect(),
            })
            .await
            .map_err(|err| format!("Failed to load show data: {err:?}"))?;

        let mut inner = self.inner.lock().await;
        inner.selected_scene_id = file
            .scene_configs
            .first()
            .map(|config| format!("{}::{}", config.scene_index, config.scene_name));
        inner.show_file_path = Some(path);
        inner.show_file_last_saved_at = Some(file.saved_at.clone());
        inner.show_file_dirty = report.removed_anything();

        for scene in report.removed_scenes {
            inner.push_log(
                super::view::LogSource::App,
                super::view::LogSeverity::Warning,
                format!("Deleted saved scene config during load: {scene}"),
            );
        }

        inner.push_log(
            super::view::LogSource::App,
            super::view::LogSeverity::Info,
            "Show file loaded".to_string(),
        );

        drop(inner);
        Ok(self.snapshot().await)
    }

    pub async fn mark_show_file_saved(&self, path: PathBuf, saved_at: String) -> AppViewState {
        let mut inner = self.inner.lock().await;
        inner.show_file_path = Some(path);
        inner.show_file_last_saved_at = Some(saved_at);
        inner.show_file_dirty = false;
        inner.push_log(
            super::view::LogSource::App,
            super::view::LogSeverity::Info,
            "Show file saved".to_string(),
        );
        drop(inner);
        self.snapshot().await
    }

    #[cfg(test)]
    pub async fn export_show_file(&self, saved_at: String) -> ShowFile {
        self.export_show_file_for_save(saved_at).await.unwrap()
    }
}

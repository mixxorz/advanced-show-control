use std::path::PathBuf;

use crate::show::show_file::{ShowFile, export_show_file, import_show_file};

use super::shell::ShellState;
use super::view::AppViewState;

impl ShellState {
    pub async fn new_show_file(&self) -> Result<AppViewState, String> {
        self.show.clear().await;

        let scenes = {
            let inner = self.inner.lock().await;
            inner
                .lv1_snapshot
                .as_ref()
                .map(|snapshot| snapshot.scene_list.clone())
                .unwrap_or_default()
        };

        if !scenes.is_empty() {
            self.show.reconcile_scene_list(scenes).await;
        }

        let selected_scene_id = self
            .show
            .get_snapshot()
            .await
            .scene_configs
            .first()
            .cloned()
            .map(|scene| scene.scene_id);

        let mut inner = self.inner.lock().await;
        inner.selected_scene_id = selected_scene_id;
        inner.show_file_path = None;
        inner.show_file_dirty = false;
        inner.show_file_last_saved_at = None;
        drop(inner);
        tracing::info!(event = "show_file_created", "New show file created");
        Ok(self.snapshot().await)
    }

    pub async fn export_show_file_for_save(&self, saved_at: String) -> Result<ShowFile, String> {
        let show = self.show.get_snapshot().await;
        Ok(export_show_file(show, saved_at))
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
        let imported = import_show_file(file, &lv1)?;
        drop(inner);

        self.show.replace_snapshot(imported.snapshot).await;

        let mut inner = self.inner.lock().await;
        inner.selected_scene_id = imported.selected_scene_id;
        inner.show_file_path = Some(path);
        inner.show_file_last_saved_at = Some(file.saved_at.clone());
        inner.show_file_dirty = imported.report.removed_anything();

        for scene in imported.report.removed_scenes {
            tracing::warn!(
                event = "show_file_scene_pruned",
                scene = %scene,
                "Skipped loading \"{scene}\" because it was not found in the current scene list."
            );
        }

        drop(inner);
        tracing::info!(event = "show_file_opened", "Show file loaded");
        Ok(self.snapshot().await)
    }

    pub async fn mark_show_file_saved(&self, path: PathBuf, saved_at: String) -> AppViewState {
        let mut inner = self.inner.lock().await;
        inner.show_file_path = Some(path);
        inner.show_file_last_saved_at = Some(saved_at);
        inner.show_file_dirty = false;
        drop(inner);
        tracing::info!(event = "show_file_saved", "Show file saved");
        self.snapshot().await
    }

    #[cfg(test)]
    pub async fn export_show_file(&self, saved_at: String) -> ShowFile {
        self.export_show_file_for_save(saved_at).await.unwrap()
    }
}

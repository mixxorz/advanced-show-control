use super::shell::{ShellInner, ShellState, snapshot_from_inner};
use super::view::FadeTarget;

impl ShellState {
    #[allow(dead_code)]
    pub async fn select_scene_config(
        &self,
        scene_id: String,
    ) -> Result<super::view::AppViewState, String> {
        let mut inner = self.inner.lock().await;

        if inner.listen_mode_active {
            return Err("Stop Listen Mode before selecting another scene".to_string());
        }

        if !inner
            .scene_fade_configs
            .iter()
            .any(|config| config.scene_id == scene_id)
        {
            return Err("Scene config not found".to_string());
        }

        inner.selected_scene_id = Some(scene_id);
        Ok(snapshot_from_inner(&inner))
    }

    #[allow(dead_code)]
    pub async fn set_scene_fade_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<super::view::AppViewState, String> {
        let mut inner = self.inner.lock().await;
        let config = inner
            .scene_fade_configs
            .iter_mut()
            .find(|config| config.scene_id == scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;

        config.fade_enabled = enabled;
        inner.show_file_dirty = true;
        Ok(snapshot_from_inner(&inner))
    }

    pub async fn set_listen_mode(&self, active: bool) -> Result<super::view::AppViewState, String> {
        let mut inner = self.inner.lock().await;

        if active {
            if inner.selected_scene_id.is_none() {
                return Err("Select a scene before starting Listen Mode".to_string());
            }

            if inner
                .lv1_snapshot
                .as_ref()
                .map(|snapshot| snapshot.channels.is_empty())
                .unwrap_or(true)
            {
                return Err("LV1 channel list is empty".to_string());
            }
        }

        inner.listen_mode_active = active;
        Ok(snapshot_from_inner(&inner))
    }

    pub async fn set_scene_duration_ms(
        &self,
        scene_id: String,
        duration_ms: u64,
    ) -> Result<super::view::AppViewState, String> {
        if !(100..=120_000).contains(&duration_ms) {
            return Err("Fade duration must be between 100 ms and 120000 ms".to_string());
        }

        let mut inner = self.inner.lock().await;
        let config = inner
            .scene_fade_configs
            .iter_mut()
            .find(|config| config.scene_id == scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;

        config.duration_ms = duration_ms;
        inner.show_file_dirty = true;
        Ok(snapshot_from_inner(&inner))
    }

    #[allow(dead_code)]
    pub async fn set_fade_target_enabled(
        &self,
        scene_id: String,
        group: i32,
        channel: i32,
        enabled: bool,
    ) -> Result<super::view::AppViewState, String> {
        let mut inner = self.inner.lock().await;
        let target = find_target_mut(&mut inner, &scene_id, group, channel)?;

        target.enabled = enabled;
        inner.show_file_dirty = true;
        Ok(snapshot_from_inner(&inner))
    }

    pub async fn remove_fade_target(
        &self,
        scene_id: &str,
        group: i32,
        channel: i32,
    ) -> Result<super::view::AppViewState, String> {
        let mut inner = self.inner.lock().await;
        let config = inner
            .scene_fade_configs
            .iter_mut()
            .find(|config| config.scene_id == scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        let before = config.fade_targets.len();
        config
            .fade_targets
            .retain(|target| !(target.group == group && target.channel == channel));

        if config.fade_targets.len() == before {
            return Err("Fade target not found".to_string());
        }

        inner.show_file_dirty = true;
        Ok(snapshot_from_inner(&inner))
    }
}

#[allow(dead_code)]
fn find_target_mut<'a>(
    inner: &'a mut ShellInner,
    scene_id: &str,
    group: i32,
    channel: i32,
) -> Result<&'a mut FadeTarget, String> {
    let config = inner
        .scene_fade_configs
        .iter_mut()
        .find(|config| config.scene_id == scene_id)
        .ok_or_else(|| "Scene config not found".to_string())?;

    config
        .fade_targets
        .iter_mut()
        .find(|target| target.group == group && target.channel == channel)
        .ok_or_else(|| "Fade target not found".to_string())
}

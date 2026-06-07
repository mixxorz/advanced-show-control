use std::collections::HashSet;

use super::shell::{ShellState, snapshot_from_inner};
use super::view::{ChannelConfig, ChannelRef};

impl ShellState {
    #[allow(dead_code)]
    pub async fn select_scene_config(
        &self,
        scene_id: String,
    ) -> Result<super::view::AppViewState, String> {
        let mut inner = self.inner.lock().await;

        if !inner
            .scene_configs
            .iter()
            .any(|config| config.scene_id == scene_id)
        {
            return Err("Scene config not found".to_string());
        }

        inner.selected_scene_id = Some(scene_id);
        Ok(snapshot_from_inner(&inner))
    }

    #[allow(dead_code)]
    pub async fn store_scene_config(
        &self,
        scene_id: String,
    ) -> Result<super::view::AppViewState, String> {
        let mut inner = self.inner.lock().await;
        let channels = inner
            .lv1_snapshot
            .as_ref()
            .map(|snapshot| snapshot.channels.clone())
            .filter(|channels| !channels.is_empty())
            .ok_or_else(|| "LV1 channel list is empty".to_string())?;

        let config = inner
            .scene_configs
            .iter_mut()
            .find(|config| config.scene_id == scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;

        let first_store = config.channel_configs.is_empty();
        let current_refs = channels
            .iter()
            .map(|channel| ChannelRef {
                group: channel.group,
                channel: channel.channel,
            })
            .collect::<Vec<_>>();
        let current_ref_set = current_refs
            .iter()
            .map(|channel| (channel.group, channel.channel))
            .collect::<HashSet<_>>();

        config.channel_configs = channels
            .iter()
            .map(|channel| ChannelConfig {
                group: channel.group,
                channel: channel.channel,
                fader_db: Some(channel.gain_db),
            })
            .collect();

        if first_store {
            config.scoped_channels = current_refs;
        } else {
            config
                .scoped_channels
                .retain(|channel| current_ref_set.contains(&(channel.group, channel.channel)));
        }

        inner.show_file_dirty = true;
        Ok(snapshot_from_inner(&inner))
    }

    pub async fn set_scene_duration_ms(
        &self,
        scene_id: String,
        duration_ms: u64,
    ) -> Result<super::view::AppViewState, String> {
        if duration_ms != 0 && !(100..=120_000).contains(&duration_ms) {
            return Err("Fade duration must be between 100 ms and 120000 ms".to_string());
        }

        let mut inner = self.inner.lock().await;
        let config = inner
            .scene_configs
            .iter_mut()
            .find(|config| config.scene_id == scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;

        config.duration_ms = duration_ms;
        inner.show_file_dirty = true;
        Ok(snapshot_from_inner(&inner))
    }

    pub async fn set_channel_scoped(
        &self,
        scene_id: String,
        group: i32,
        channel: i32,
        scoped: bool,
    ) -> Result<super::view::AppViewState, String> {
        let mut inner = self.inner.lock().await;
        let config = inner
            .scene_configs
            .iter_mut()
            .find(|config| config.scene_id == scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;

        if !config
            .channel_configs
            .iter()
            .any(|entry| entry.group == group && entry.channel == channel)
        {
            return Err("Channel config not found".to_string());
        }

        config
            .scoped_channels
            .retain(|entry| !(entry.group == group && entry.channel == channel));
        if scoped {
            config.scoped_channels.push(ChannelRef { group, channel });
        }

        inner.show_file_dirty = true;
        Ok(snapshot_from_inner(&inner))
    }

    #[allow(dead_code)]
    pub async fn set_all_channels_scoped(
        &self,
        scene_id: String,
        scoped: bool,
    ) -> Result<super::view::AppViewState, String> {
        let mut inner = self.inner.lock().await;
        let config = inner
            .scene_configs
            .iter_mut()
            .find(|config| config.scene_id == scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;

        config.scoped_channels = if scoped {
            config
                .channel_configs
                .iter()
                .map(|entry| ChannelRef {
                    group: entry.group,
                    channel: entry.channel,
                })
                .collect()
        } else {
            Vec::new()
        };

        inner.show_file_dirty = true;
        Ok(snapshot_from_inner(&inner))
    }
}

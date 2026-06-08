use crate::lv1::types::ChannelInfo;

use super::state::ShowState;
use super::types::{ChannelConfig, ChannelRef, SceneConfig};

impl ShowState {
    pub fn store_scene_config(&mut self, scene_id: &str, channels: &[ChannelInfo]) -> Result<bool, String> {
        if channels.is_empty() {
            return Err("LV1 channel list is empty".to_string());
        }
        let snapshot = SceneConfig {
            scene_id: scene_id.to_string(),
            duration_ms: self.get_scene_config(scene_id).map(|scene| scene.duration_ms).unwrap_or(1_000),
            channels: channels
                .iter()
                .map(|channel| ChannelConfig {
                    channel: ChannelRef { group: channel.group, channel: channel.channel },
                    scoped: true,
                    target_db: channel.gain_db,
                })
                .collect(),
        };
        match self.scene_configs.iter_mut().find(|scene| scene.scene_id == scene_id) {
            Some(existing) => {
                if *existing == snapshot { Ok(false) } else { *existing = snapshot; Ok(true) }
            }
            None => {
                self.scene_configs.push(snapshot);
                Ok(true)
            }
        }
    }

    pub fn set_scene_duration_ms(&mut self, scene_id: &str, duration_ms: u64) -> Result<bool, String> {
        if !(100..=120_000).contains(&duration_ms) {
            return Err("Fade duration must be between 100 ms and 120000 ms".to_string());
        }
        let scene = self.get_scene_config_mut(scene_id).ok_or_else(|| "Scene config not found".to_string())?;
        if scene.duration_ms == duration_ms { Ok(false) } else { scene.duration_ms = duration_ms; Ok(true) }
    }

    pub fn set_channel_scoped(&mut self, scene_id: &str, group: i32, channel: i32, scoped: bool) -> Result<bool, String> {
        let scene = self.get_scene_config_mut(scene_id).ok_or_else(|| "Scene config not found".to_string())?;
        let channel = scene.channels.iter_mut().find(|entry| entry.channel.group == group && entry.channel.channel == channel).ok_or_else(|| "Channel config not found".to_string())?;
        if channel.scoped == scoped { Ok(false) } else { channel.scoped = scoped; Ok(true) }
    }

    pub fn set_all_channels_scoped(&mut self, scene_id: &str, scoped: bool) -> Result<bool, String> {
        let scene = self.get_scene_config_mut(scene_id).ok_or_else(|| "Scene config not found".to_string())?;
        let mut changed = false;
        for channel in &mut scene.channels { if channel.scoped != scoped { channel.scoped = scoped; changed = true; } }
        Ok(changed)
    }
}

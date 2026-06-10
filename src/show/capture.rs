use crate::lv1::types::ChannelInfo;

use super::state::ShowState;
use super::types::{ChannelConfig, ChannelRef, SceneConfig, parse_scene_id};

impl ShowState {
    pub fn store_scene_config(
        &mut self,
        scene_id: &str,
        channels: &[ChannelInfo],
    ) -> Result<bool, String> {
        if channels.is_empty() {
            return Err("LV1 channel list is empty".to_string());
        }
        let current_refs: Vec<ChannelRef> = channels
            .iter()
            .map(|channel| ChannelRef {
                group: channel.group,
                channel: channel.channel,
            })
            .collect();
        let scoped_channels = self
            .get_scene_config(scene_id)
            .map(|scene| {
                if scene.scoped_channels.is_empty() {
                    current_refs.clone()
                } else {
                    scene
                        .scoped_channels
                        .into_iter()
                        .filter(|scoped| current_refs.iter().any(|current| current == scoped))
                        .collect()
                }
            })
            .unwrap_or_else(|| current_refs.clone());
        let previous = self.get_scene_config(scene_id);
        let (scene_index, scene_name) =
            parse_scene_id(scene_id).unwrap_or_else(|_| (0, String::new()));
        let snapshot = SceneConfig {
            scene_id: scene_id.to_string(),
            scene_index,
            scene_name,
            duration_ms: self
                .get_scene_config(scene_id)
                .map(|scene| scene.duration_ms)
                .unwrap_or(1_000),
            channel_configs: channels
                .iter()
                .map(|channel| {
                    let previous_channel = previous.as_ref().and_then(|scene| {
                        scene.channel_configs.iter().find(|entry| {
                            entry.group == channel.group && entry.channel == channel.channel
                        })
                    });
                    ChannelConfig {
                        group: channel.group,
                        channel: channel.channel,
                        fader_db: Some(channel.gain_db),
                        pan: channel
                            .pan
                            .or_else(|| previous_channel.and_then(|entry| entry.pan)),
                        balance: channel
                            .balance
                            .or_else(|| previous_channel.and_then(|entry| entry.balance)),
                        width: channel
                            .width
                            .or_else(|| previous_channel.and_then(|entry| entry.width)),
                        pan_mode: channel
                            .pan_mode
                            .clone()
                            .or_else(|| previous_channel.and_then(|entry| entry.pan_mode.clone())),
                    }
                })
                .collect(),
            scoped_channels,
            scope_toggles: self
                .get_scene_config(scene_id)
                .map(|scene| scene.scope_toggles)
                .unwrap_or_default(),
        };
        match self
            .scene_configs
            .iter_mut()
            .find(|scene| scene.scene_id == scene_id)
        {
            Some(existing) => {
                if *existing == snapshot {
                    Ok(false)
                } else {
                    *existing = snapshot;
                    Ok(true)
                }
            }
            None => {
                self.scene_configs.push(snapshot);
                Ok(true)
            }
        }
    }

    pub fn set_scene_duration_ms(
        &mut self,
        scene_id: &str,
        duration_ms: u64,
    ) -> Result<bool, String> {
        if duration_ms != 0 && !(100..=120_000).contains(&duration_ms) {
            return Err("Fade duration must be 0 or between 100 ms and 120000 ms".to_string());
        }
        let scene = self
            .get_scene_config_mut(scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        if scene.duration_ms == duration_ms {
            Ok(false)
        } else {
            scene.duration_ms = duration_ms;
            Ok(true)
        }
    }

    pub fn set_channel_scoped(
        &mut self,
        scene_id: &str,
        group: i32,
        channel: i32,
        scoped: bool,
    ) -> Result<bool, String> {
        let scene = self
            .get_scene_config_mut(scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        let channel_exists = scene
            .channel_configs
            .iter()
            .any(|entry| entry.group == group && entry.channel == channel);
        if !channel_exists {
            return Err("Channel config not found".to_string());
        }
        let ref_exists = scene
            .scoped_channels
            .iter()
            .any(|entry| entry.group == group && entry.channel == channel);
        match (scoped, ref_exists) {
            (true, false) => {
                scene.scoped_channels.push(ChannelRef { group, channel });
                Ok(true)
            }
            (false, true) => {
                scene
                    .scoped_channels
                    .retain(|entry| !(entry.group == group && entry.channel == channel));
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn set_all_channels_scoped(
        &mut self,
        scene_id: &str,
        scoped: bool,
    ) -> Result<bool, String> {
        let scene = self
            .get_scene_config_mut(scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        let mut changed = false;
        let refs: Vec<ChannelRef> = scene
            .channel_configs
            .iter()
            .map(|entry| ChannelRef {
                group: entry.group,
                channel: entry.channel,
            })
            .collect();
        if scoped {
            // Check if all channels are already scoped (regardless of order)
            let all_scoped = refs.iter().all(|ref_channel| {
                scene
                    .scoped_channels
                    .iter()
                    .any(|scoped_channel| scoped_channel == ref_channel)
            });
            if !all_scoped || scene.scoped_channels.len() != refs.len() {
                // Either not all channels are scoped, or there are extra scoped channels
                scene.scoped_channels = refs;
                changed = true;
            }
        } else if !scene.scoped_channels.is_empty() {
            scene.scoped_channels.clear();
            changed = true;
        }
        Ok(changed)
    }

    pub fn set_scene_scope_faders_enabled(
        &mut self,
        scene_id: &str,
        enabled: bool,
    ) -> Result<bool, String> {
        let scene = self
            .get_scene_config_mut(scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        if scene.scope_toggles.faders == enabled {
            Ok(false)
        } else {
            scene.scope_toggles.faders = enabled;
            Ok(true)
        }
    }

    pub fn set_scene_scope_pan_enabled(
        &mut self,
        scene_id: &str,
        enabled: bool,
    ) -> Result<bool, String> {
        let scene = self
            .get_scene_config_mut(scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        if scene.scope_toggles.pan == enabled {
            Ok(false)
        } else {
            scene.scope_toggles.pan = enabled;
            Ok(true)
        }
    }
}

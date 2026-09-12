use super::state::ScenesState;
use crate::lv1::ChannelInfo;
use crate::scenes::{ChannelConfig, ChannelRef, SceneConfig, is_supported_scope_group};
use uuid::Uuid;
impl ScenesState {
    /**
     * @cc [owner:mixxorz,label:safety] capture-prerequisites
     * Capture MUST fail without mutation when the live channel list is empty, the scene config is
     * missing, or the scene config is unlinked.
     */
    /**
     * @cc [owner:mixxorz,label:product] capture-scope-preservation
     * Capture MUST preserve an empty scope as empty and otherwise only remove scoped channels absent
     * from the supplied live snapshot.
     */
    /**
     * @cc [owner:mixxorz,label:product] capture-preserves-scene-policy
     * Capture MUST keep the scene's durable UUID, linked index/name, duration, and scope toggles;
     * it MUST refresh values from the supplied live channels, retain prior pan-family values when
     * unavailable live, and restrict an existing nonempty scope to channels still present.
     */
    pub(crate) fn store_scene_config(
        &mut self,
        internal_scene_id: Uuid,
        channels: &[ChannelInfo],
    ) -> Result<bool, String> {
        if channels.is_empty() {
            return Err("LV1 channel list is empty".to_string());
        }
        let previous = self
            .get_scene_config(internal_scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        let Some(scene_index) = previous.scene_index else {
            return Err("Store blocked: scene is unlinked".to_string());
        };
        let current_refs: Vec<ChannelRef> = channels
            .iter()
            .map(|channel| ChannelRef {
                group: channel.group,
                channel: channel.channel,
            })
            .collect();
        let scoped_channels = previous
            .scoped_channels
            .iter()
            .filter(|scoped| current_refs.iter().any(|current| current == *scoped))
            .cloned()
            .collect();
        let snapshot = SceneConfig {
            internal_scene_id,
            scene_index: Some(scene_index),
            scene_name: previous.scene_name.clone(),
            duration_ms: self
                .get_scene_config(internal_scene_id)
                .map(|scene| scene.duration_ms)
                .unwrap_or(1_000),
            channel_configs: channels
                .iter()
                .map(|channel| {
                    let previous_channel = previous.channel_configs.iter().find(|entry| {
                        entry.group == channel.group && entry.channel == channel.channel
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
                .get_scene_config(internal_scene_id)
                .map(|scene| scene.scope_toggles)
                .unwrap_or_default(),
        };
        Ok(self.upsert_scene_config(snapshot))
    }

    pub(crate) fn set_scene_duration_ms(
        &mut self,
        internal_scene_id: Uuid,
        duration_ms: u64,
    ) -> Result<bool, String> {
        if duration_ms != 0 && !(100..=120_000).contains(&duration_ms) {
            return Err("Fade duration must be 0 or between 100 ms and 120000 ms".to_string());
        }
        let scene = self
            .get_scene_config_mut(internal_scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        if scene.duration_ms == duration_ms {
            Ok(false)
        } else {
            scene.duration_ms = duration_ms;
            Ok(true)
        }
    }

    pub(crate) fn set_channel_scoped(
        &mut self,
        internal_scene_id: Uuid,
        group: i32,
        channel: i32,
        scoped: bool,
    ) -> Result<bool, String> {
        let scene = self
            .get_scene_config_mut(internal_scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        if scoped && !is_supported_scope_group(group) {
            return Err(format!(
                "Channel group {group} is not supported for scene scope"
            ));
        }
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

    pub(crate) fn set_all_channels_scoped(
        &mut self,
        internal_scene_id: Uuid,
        scoped: bool,
    ) -> Result<bool, String> {
        let scene = self
            .get_scene_config_mut(internal_scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        let mut changed = false;
        let refs: Vec<ChannelRef> = scene
            .channel_configs
            .iter()
            .filter(|entry| is_supported_scope_group(entry.group))
            .map(|entry| ChannelRef {
                group: entry.group,
                channel: entry.channel,
            })
            .collect();
        if scoped {
            let all_scoped = refs.iter().all(|ref_channel| {
                scene
                    .scoped_channels
                    .iter()
                    .any(|scoped_channel| scoped_channel == ref_channel)
            });
            if !all_scoped || scene.scoped_channels.len() != refs.len() {
                scene.scoped_channels = refs;
                changed = true;
            }
        } else if !scene.scoped_channels.is_empty() {
            scene.scoped_channels.clear();
            changed = true;
        }
        Ok(changed)
    }

    pub(crate) fn set_scene_scope_faders_enabled(
        &mut self,
        internal_scene_id: Uuid,
        enabled: bool,
    ) -> Result<bool, String> {
        let scene = self
            .get_scene_config_mut(internal_scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        if scene.scope_toggles.faders == enabled {
            Ok(false)
        } else {
            scene.scope_toggles.faders = enabled;
            Ok(true)
        }
    }

    pub(crate) fn set_scene_scope_pan_enabled(
        &mut self,
        internal_scene_id: Uuid,
        enabled: bool,
    ) -> Result<bool, String> {
        let scene = self
            .get_scene_config_mut(internal_scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        if scene.scope_toggles.pan == enabled {
            Ok(false)
        } else {
            scene.scope_toggles.pan = enabled;
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenes::{SceneDocument, SceneScopeToggles};

    fn state_with_supported_and_unknown_channels() -> (ScenesState, Uuid) {
        let scene_id = Uuid::from_u128(0x11111111111141118111111111111111);
        let channels = [0, 24]
            .map(|group| ChannelConfig {
                group,
                channel: 0,
                fader_db: Some(0.0),
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            })
            .to_vec();
        let mut state = ScenesState::default();
        state.replace_snapshot(SceneDocument {
            scene_configs: vec![SceneConfig {
                internal_scene_id: scene_id,
                scene_index: Some(0),
                scene_name: "Scene".to_string(),
                duration_ms: 1_000,
                channel_configs: channels,
                scoped_channels: Vec::new(),
                scope_toggles: SceneScopeToggles::default(),
            }],
            selected_scene_internal_id: None,
        });
        (state, scene_id)
    }

    #[test]
    fn all_scopes_only_channel_groups_exposed_by_the_scope_editor() {
        let (mut state, scene_id) = state_with_supported_and_unknown_channels();

        assert!(state.set_all_channels_scoped(scene_id, true).unwrap());

        assert_eq!(
            state.get_scene_config(scene_id).unwrap().scoped_channels,
            vec![ChannelRef {
                group: 0,
                channel: 0,
            }]
        );
    }

    #[test]
    fn unknown_channel_groups_cannot_be_added_to_scope() {
        let (mut state, scene_id) = state_with_supported_and_unknown_channels();

        let error = state.set_channel_scoped(scene_id, 24, 0, true).unwrap_err();

        assert_eq!(error, "Channel group 24 is not supported for scene scope");
        assert!(
            state
                .get_scene_config(scene_id)
                .unwrap()
                .scoped_channels
                .is_empty()
        );
    }
}

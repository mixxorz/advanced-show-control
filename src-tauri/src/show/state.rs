use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};
use crate::lv1::Lv1StateSnapshot;
use crate::scenes::ScenesState;
use crate::show::show_file::{ShowFile, export_show_file};

use super::types::ShowDocument;
use crate::scenes::{SceneConfig, SceneDocument, align_scene_configs};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShowState {
    lockout: bool,
    scene_configs: Vec<SceneConfig>,
    cued_scene_internal_id: Option<Uuid>,
    selected_scene_internal_id: Option<String>,
    show_file_path: Option<std::path::PathBuf>,
    show_file_dirty: bool,
    show_file_last_saved_at: Option<String>,
    discovered_lv1_systems: Vec<DiscoveredLv1System>,
    connected_lv1_identity: Option<Lv1SystemIdentity>,
    pending_lv1_identity: Option<Lv1SystemIdentity>,
    reconnect: ReconnectState,
    last_event_at: Option<String>,
}

impl ShowState {
    pub(crate) fn reset_for_new_show(&mut self, lv1: Option<&Lv1StateSnapshot>) -> Option<String> {
        self.clear();
        if let Some(lv1) = lv1
            && !lv1.scene_list.is_empty()
        {
            self.scene_configs = align_scene_configs(self.scene_configs.clone(), &lv1.scene_list);
        }
        self.selected_scene_internal_id = self
            .scene_configs
            .first()
            .map(|scene| scene.internal_scene_id.to_string());
        self.show_file_path = None;
        self.show_file_dirty = false;
        self.show_file_last_saved_at = None;
        self.selected_scene_internal_id.clone()
    }

    pub(crate) fn mark_saved(&mut self, path: std::path::PathBuf, saved_at: String) {
        self.show_file_path = Some(path);
        self.show_file_last_saved_at = Some(saved_at);
        self.show_file_dirty = false;
    }

    pub(crate) fn set_selected_scene_internal_id(
        &mut self,
        selected_scene_internal_id: Option<String>,
    ) -> bool {
        if self.selected_scene_internal_id == selected_scene_internal_id {
            return false;
        }
        self.selected_scene_internal_id = selected_scene_internal_id;
        true
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.show_file_dirty = true;
    }

    pub(crate) fn set_discovered_lv1_systems(&mut self, systems: Vec<DiscoveredLv1System>) -> bool {
        if self.discovered_lv1_systems == systems {
            false
        } else {
            self.discovered_lv1_systems = systems;
            true
        }
    }

    pub(crate) fn set_pending_lv1_identity(&mut self, identity: Option<Lv1SystemIdentity>) -> bool {
        if self.pending_lv1_identity == identity {
            false
        } else {
            self.pending_lv1_identity = identity;
            true
        }
    }

    pub(crate) fn establish_connected_lv1_identity(&mut self, identity: Lv1SystemIdentity) -> bool {
        let changed = self.connected_lv1_identity.as_ref() != Some(&identity)
            || self.pending_lv1_identity.is_some();
        if changed {
            self.connected_lv1_identity = Some(identity);
            self.pending_lv1_identity = None;
        }
        changed
    }

    pub(crate) fn clear_connected_lv1_identity(&mut self) -> bool {
        if self.connected_lv1_identity.is_none() {
            false
        } else {
            self.connected_lv1_identity = None;
            true
        }
    }

    pub(crate) fn set_reconnect_state(&mut self, reconnect: ReconnectState) -> bool {
        if self.reconnect == reconnect {
            false
        } else {
            self.reconnect = reconnect;
            true
        }
    }

    pub(crate) fn handle_runtime_disconnected(&mut self, _reason: String) -> bool {
        let mut changed = false;
        if self.connected_lv1_identity.take().is_some() {
            changed = true;
        }
        if self.pending_lv1_identity.take().is_some() {
            changed = true;
        }
        let next = ReconnectState {
            active: false,
            attempt: 0,
        };
        if self.reconnect != next {
            self.reconnect = next;
            changed = true;
        }
        let timestamp = crate::time::current_timestamp_millis();
        if self.last_event_at.as_ref() != Some(&timestamp) {
            self.last_event_at = Some(timestamp);
            changed = true;
        }
        changed
    }

    pub(crate) fn lockout(&self) -> bool {
        self.lockout
    }

    pub(crate) fn current_show_file_path(&self) -> Option<std::path::PathBuf> {
        self.show_file_path.clone()
    }

    pub(crate) fn export_show_file(&self, saved_at: String) -> ShowFile {
        export_show_file(
            SceneDocument {
                scene_configs: self.scene_configs.clone(),
                cued_scene_internal_id: self.cued_scene_internal_id,
                selected_scene_internal_id: self.selected_scene_internal_id.clone(),
            },
            self.lockout,
            saved_at,
        )
    }

    pub(crate) fn scene_configs(&self) -> &[SceneConfig] {
        &self.scene_configs
    }

    pub(crate) fn replace_scene_configs_if_changed(&mut self, next: Vec<SceneConfig>) -> bool {
        if self.scene_configs == next {
            false
        } else {
            self.scene_configs = next;
            self.clear_missing_cue();
            true
        }
    }

    fn scenes_state(&self) -> ScenesState {
        let mut scenes = ScenesState::default();
        scenes.replace_snapshot(SceneDocument {
            scene_configs: self.scene_configs.clone(),
            cued_scene_internal_id: self.cued_scene_internal_id,
            selected_scene_internal_id: self.selected_scene_internal_id.clone(),
        });
        scenes
    }

    fn sync_from_scenes_state(&mut self, scenes: ScenesState) {
        let snapshot = scenes.snapshot();
        self.scene_configs = snapshot.scene_configs;
        self.cued_scene_internal_id = snapshot.cued_scene_internal_id;
        self.selected_scene_internal_id = snapshot.selected_scene_internal_id;
    }

    pub fn store_scene_config(
        &mut self,
        internal_scene_id: Uuid,
        channels: &[crate::lv1::ChannelInfo],
    ) -> Result<bool, String> {
        let mut scenes = self.scenes_state();
        let changed = scenes.store_scene_config(internal_scene_id, channels)?;
        self.sync_from_scenes_state(scenes);
        Ok(changed)
    }

    pub fn set_scene_duration_ms(
        &mut self,
        internal_scene_id: Uuid,
        duration_ms: u64,
    ) -> Result<bool, String> {
        let mut scenes = self.scenes_state();
        let changed = scenes.set_scene_duration_ms(internal_scene_id, duration_ms)?;
        self.sync_from_scenes_state(scenes);
        Ok(changed)
    }

    pub fn set_channel_scoped(
        &mut self,
        internal_scene_id: Uuid,
        group: i32,
        channel: i32,
        scoped: bool,
    ) -> Result<bool, String> {
        let mut scenes = self.scenes_state();
        let changed = scenes.set_channel_scoped(internal_scene_id, group, channel, scoped)?;
        self.sync_from_scenes_state(scenes);
        Ok(changed)
    }

    pub fn set_all_channels_scoped(
        &mut self,
        internal_scene_id: Uuid,
        scoped: bool,
    ) -> Result<bool, String> {
        let mut scenes = self.scenes_state();
        let changed = scenes.set_all_channels_scoped(internal_scene_id, scoped)?;
        self.sync_from_scenes_state(scenes);
        Ok(changed)
    }

    pub fn set_scene_scope_faders_enabled(
        &mut self,
        internal_scene_id: Uuid,
        enabled: bool,
    ) -> Result<bool, String> {
        let mut scenes = self.scenes_state();
        let changed = scenes.set_scene_scope_faders_enabled(internal_scene_id, enabled)?;
        self.sync_from_scenes_state(scenes);
        Ok(changed)
    }

    pub fn set_scene_scope_pan_enabled(
        &mut self,
        internal_scene_id: Uuid,
        enabled: bool,
    ) -> Result<bool, String> {
        let mut scenes = self.scenes_state();
        let changed = scenes.set_scene_scope_pan_enabled(internal_scene_id, enabled)?;
        self.sync_from_scenes_state(scenes);
        Ok(changed)
    }

    pub fn snapshot(&self) -> ShowDocument {
        ShowDocument {
            lockout: self.lockout,
            scene_configs: self.scene_configs.clone(),
            cued_scene_internal_id: self.cued_scene_internal_id,
        }
    }

    pub fn projection_state(&self) -> super::events::ShowProjectionState {
        let show_file_name = self
            .show_file_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| "Untitled Session".to_string());

        super::events::ShowProjectionState {
            lockout: self.lockout,
            scene_configs: self.scene_configs.clone(),
            cued_scene_internal_id: self.cued_scene_internal_id.map(|id| id.to_string()),
            selected_scene_internal_id: self.selected_scene_internal_id.clone(),
            show_file_path: self.show_file_path.clone(),
            show_file_name,
            show_file_dirty: self.show_file_dirty,
            show_file_last_saved_at: self.show_file_last_saved_at.clone(),
            discovered_lv1_systems: self.discovered_lv1_systems.clone(),
            connected_lv1_identity: self.connected_lv1_identity.clone(),
            pending_lv1_identity: self.pending_lv1_identity.clone(),
            reconnect: self.reconnect.clone(),
            last_event_at: self.last_event_at.clone(),
        }
    }

    pub fn cue_scene(&mut self, internal_scene_id: Uuid) -> Result<bool, String> {
        if !self
            .scene_configs
            .iter()
            .any(|scene| scene.internal_scene_id == internal_scene_id)
        {
            return Err("Scene config not found".to_string());
        }

        let next = Some(internal_scene_id);
        if self.cued_scene_internal_id == next {
            return Ok(false);
        }

        self.cued_scene_internal_id = next;
        Ok(true)
    }

    pub fn link_scene_config(
        &mut self,
        source_internal_scene_id: Uuid,
        target: &crate::lv1::SceneListEntry,
        overwrite_existing: bool,
    ) -> Result<bool, String> {
        let source = self
            .scene_configs
            .iter()
            .find(|scene| scene.internal_scene_id == source_internal_scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        if source.scene_index.is_some() {
            return Err("Link blocked: source scene is already linked".to_string());
        }
        if let Some(target_index) = self
            .scene_configs
            .iter()
            .position(|scene| scene.scene_index == Some(target.index))
        {
            if self.scene_configs[target_index].internal_scene_id == source_internal_scene_id {
                return Ok(false);
            }
            if !overwrite_existing {
                return Err("Link blocked: target scene already has a config".to_string());
            }
            let removed_internal_scene_id = self.scene_configs[target_index].internal_scene_id;
            self.scene_configs.remove(target_index);
            if self.selected_scene_internal_id.as_deref()
                == Some(&removed_internal_scene_id.to_string())
            {
                self.selected_scene_internal_id = None;
            }
            if self.cued_scene_internal_id == Some(removed_internal_scene_id) {
                self.cued_scene_internal_id = None;
            }
        }
        let source = self
            .scene_configs
            .iter_mut()
            .find(|scene| scene.internal_scene_id == source_internal_scene_id)
            .ok_or_else(|| "Scene config not found".to_string())?;
        source.scene_index = Some(target.index);
        source.scene_name = target.name.clone();
        self.sort_scene_configs();
        Ok(true)
    }

    pub fn delete_scene_config(&mut self, internal_scene_id: Uuid) -> Result<bool, String> {
        let Some(index) = self
            .scene_configs
            .iter()
            .position(|scene| scene.internal_scene_id == internal_scene_id)
        else {
            return Err("Scene config not found".to_string());
        };
        self.scene_configs.remove(index);
        if self.selected_scene_internal_id.as_deref() == Some(&internal_scene_id.to_string()) {
            self.selected_scene_internal_id = None;
        }
        if self.cued_scene_internal_id == Some(internal_scene_id) {
            self.cued_scene_internal_id = None;
        }
        Ok(true)
    }

    pub fn replace_snapshot(&mut self, snapshot: ShowDocument) {
        self.lockout = snapshot.lockout;
        self.scene_configs = snapshot.scene_configs;
        self.cued_scene_internal_id = snapshot.cued_scene_internal_id;
    }

    pub fn clear(&mut self) {
        self.lockout = false;
        self.scene_configs.clear();
        self.cued_scene_internal_id = None;
    }

    fn clear_missing_cue(&mut self) {
        if let Some(cued_scene_internal_id) = self.cued_scene_internal_id
            && !self
                .scene_configs
                .iter()
                .any(|scene| scene.internal_scene_id == cued_scene_internal_id)
        {
            self.cued_scene_internal_id = None;
        }
    }

    pub fn get_scene_config(&self, internal_scene_id: Uuid) -> Option<SceneConfig> {
        self.scene_configs
            .iter()
            .find(|scene| scene.internal_scene_id == internal_scene_id)
            .cloned()
    }

    pub fn set_lockout(&mut self, enabled: bool) -> bool {
        if self.lockout == enabled {
            false
        } else {
            self.lockout = enabled;
            true
        }
    }

    fn sort_scene_configs(&mut self) {
        self.scene_configs.sort_by_key(|scene| {
            (
                scene.scene_index.is_none(),
                scene.scene_index.unwrap_or(i32::MAX),
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::{ChannelInfo, PanMode};
    use crate::scenes::{ChannelConfig, SceneScopeToggles};
    use uuid::Uuid;

    fn channel(group: i32, channel: i32, name: &str, gain_db: f64) -> ChannelInfo {
        ChannelInfo {
            group,
            channel,
            name: name.to_string(),
            gain_db,
            muted: false,
            pan: None,
            balance: None,
            width: None,
            pan_mode: None,
        }
    }

    fn test_uuid(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn scene_config(
        scene_index: i32,
        scene_name: &str,
        duration_ms: u64,
        channels: Vec<ChannelConfig>,
        internal_scene_id: Uuid,
    ) -> SceneConfig {
        SceneConfig {
            internal_scene_id,
            scene_index: Some(scene_index),
            scene_name: scene_name.to_string(),
            duration_ms,
            channel_configs: channels,
            scoped_channels: vec![],
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    #[test]
    fn invalid_duration_rejected() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                1,
                "scene-1",
                1_000,
                vec![],
                Uuid::from_u128(0x88888888888848888888888888888888),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        let err = state
            .set_scene_duration_ms(test_uuid(0x88888888888848888888888888888888), 99)
            .unwrap_err();

        assert_eq!(
            err,
            "Fade duration must be 0 or between 100 ms and 120000 ms"
        );
    }

    #[test]
    fn channel_scope_mutation_toggles_and_reports_noop() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                1,
                "scene-1",
                1_000,
                vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-9.0),
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
                test_uuid(0x99999999999949999999999999999999),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        let internal_scene_id = state.scene_configs[0].internal_scene_id;

        assert!(
            state
                .set_channel_scoped(internal_scene_id, 0, 1, true)
                .unwrap()
        );
        assert!(
            !state
                .set_channel_scoped(internal_scene_id, 0, 1, true)
                .unwrap()
        );
    }

    #[test]
    fn store_scene_config_snapshots_current_channels_and_scopes() {
        let internal_scene_id = test_uuid(0x11111111111141118111111111111111);
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(1, "scene-1", 1_000, vec![], internal_scene_id)],
            cued_scene_internal_id: None,
            ..Default::default()
        };
        let channels = vec![ChannelInfo {
            group: 0,
            channel: 1,
            name: "Ch 1".to_string(),
            gain_db: -7.5,
            muted: false,
            pan: None,
            balance: None,
            width: None,
            pan_mode: None,
        }];

        assert!(
            state
                .store_scene_config(internal_scene_id, &channels)
                .unwrap()
        );

        let snapshot = state.snapshot();
        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(snapshot.scene_configs[0].scene_index, Some(1));
        assert_eq!(snapshot.scene_configs[0].scene_name, "scene-1");
        assert_eq!(snapshot.scene_configs[0].channel_configs[0].group, 0);
        assert_eq!(snapshot.scene_configs[0].channel_configs[0].channel, 1);
        assert_eq!(
            snapshot.scene_configs[0].channel_configs[0].fader_db,
            Some(-7.5)
        );
        assert_eq!(snapshot.scene_configs[0].scoped_channels[0].group, 0);
        assert_eq!(snapshot.scene_configs[0].scoped_channels[0].channel, 1);
    }

    #[test]
    fn store_scene_config_rejects_missing_scene_config() {
        let mut state = ShowState::default();
        let channels = vec![channel(0, 1, "Lead", -7.5)];
        let internal_scene_id = test_uuid(0xabcabcabcabc4abc8abcabcabcabcabc);

        let err = state
            .store_scene_config(internal_scene_id, &channels)
            .unwrap_err();

        assert_eq!(err, "Scene config not found");
    }

    #[test]
    fn store_scene_config_rejects_unlinked_scene() {
        let id = test_uuid(0x55555555555545558555555555555555);
        let mut state = ShowState::default();
        state.replace_snapshot(ShowDocument {
            lockout: false,
            cued_scene_internal_id: None,
            scene_configs: vec![SceneConfig {
                internal_scene_id: id,
                scene_index: None,
                scene_name: "Old Verse".to_string(),
                duration_ms: 1_000,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: SceneScopeToggles::default(),
            }],
        });

        let err = state
            .store_scene_config(id, &[channel(0, 1, "Lead", -6.0)])
            .unwrap_err();

        assert_eq!(err, "Store blocked: scene is unlinked");
    }

    #[test]
    fn store_scene_config_preserves_existing_pan_family_fields() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                1,
                "scene-1",
                1_000,
                vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-9.0),
                    pan: Some(-12.0),
                    balance: Some(3.0),
                    width: Some(1.2),
                    pan_mode: Some(PanMode::Stereo),
                }],
                test_uuid(0xaaaaaaaaaaaa4aaaaaaaaaaaaaaaaaaa),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        let internal_scene_id = state.scene_configs[0].internal_scene_id;

        assert!(
            state
                .store_scene_config(internal_scene_id, &[channel(0, 1, "Lead", -6.0)])
                .unwrap()
        );

        let stored = &state.scene_configs[0].channel_configs[0];
        assert_eq!(stored.fader_db, Some(-6.0));
        assert_eq!(stored.pan, Some(-12.0));
        assert_eq!(stored.balance, Some(3.0));
        assert_eq!(stored.width, Some(1.2));
        assert_eq!(stored.pan_mode, Some(PanMode::Stereo));
    }

    #[test]
    fn store_scene_config_updates_fresh_pan_family_fields_when_available() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                1,
                "scene-1",
                1_000,
                vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-9.0),
                    pan: Some(-12.0),
                    balance: Some(3.0),
                    width: Some(1.2),
                    pan_mode: Some(PanMode::Stereo),
                }],
                test_uuid(0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        let internal_scene_id = state.scene_configs[0].internal_scene_id;

        assert!(
            state
                .store_scene_config(
                    internal_scene_id,
                    &[ChannelInfo {
                        group: 0,
                        channel: 1,
                        name: "Lead".to_string(),
                        gain_db: -6.0,
                        muted: false,
                        pan: Some(0.25),
                        balance: Some(-0.5),
                        width: Some(1.0),
                        pan_mode: Some(PanMode::Mono),
                    }]
                )
                .unwrap()
        );

        let stored = &state.scene_configs[0].channel_configs[0];
        assert_eq!(stored.fader_db, Some(-6.0));
        assert_eq!(stored.pan, Some(0.25));
        assert_eq!(stored.balance, Some(-0.5));
        assert_eq!(stored.width, Some(1.0));
        assert_eq!(stored.pan_mode, Some(PanMode::Mono));
    }

    #[test]
    fn store_scene_config_preserves_existing_pan_family_fields_when_live_values_missing() {
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(
                1,
                "scene-1",
                1_000,
                vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-9.0),
                    pan: Some(-12.0),
                    balance: Some(3.0),
                    width: Some(1.2),
                    pan_mode: Some(PanMode::Stereo),
                }],
                test_uuid(0xcccccccccccccccccccccccccccccccc),
            )],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        let internal_scene_id = state.scene_configs[0].internal_scene_id;

        assert!(
            state
                .store_scene_config(
                    internal_scene_id,
                    &[ChannelInfo {
                        group: 0,
                        channel: 1,
                        name: "Lead".to_string(),
                        gain_db: -6.0,
                        muted: false,
                        pan: None,
                        balance: None,
                        width: Some(2.0),
                        pan_mode: None,
                    }]
                )
                .unwrap()
        );

        let stored = &state.scene_configs[0].channel_configs[0];
        assert_eq!(stored.fader_db, Some(-6.0));
        assert_eq!(stored.pan, Some(-12.0));
        assert_eq!(stored.balance, Some(3.0));
        assert_eq!(stored.width, Some(2.0));
        assert_eq!(stored.pan_mode, Some(PanMode::Stereo));
    }

    #[test]
    fn store_scene_config_defaults_fader_scope_enabled() {
        let internal_scene_id = test_uuid(0xdadadadadada4ada8adadadadadadada);
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(1, "scene-1", 1_000, vec![], internal_scene_id)],
            cued_scene_internal_id: None,
            ..Default::default()
        };
        let changed = state
            .store_scene_config(internal_scene_id, &[channel(0, 1, "Lead", -6.0)])
            .unwrap();

        assert!(changed);
        assert!(state.scene_configs[0].scope_toggles.faders);
    }

    #[test]
    fn store_scene_config_preserves_fader_scope_toggle() {
        let internal_scene_id = test_uuid(0xdddddddddddd4ddddddddddddddddddd);
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(1, "scene-1", 1_000, vec![], internal_scene_id)],
            cued_scene_internal_id: None,
            ..Default::default()
        };
        assert!(
            state
                .set_scene_scope_faders_enabled(internal_scene_id, false)
                .unwrap()
        );

        state
            .store_scene_config(internal_scene_id, &[channel(0, 1, "Lead", -3.0)])
            .unwrap();

        assert!(!state.scene_configs[0].scope_toggles.faders);
    }

    #[test]
    fn scene_scope_fader_toggle_mutation_reports_noop() {
        let internal_scene_id = test_uuid(0xeeeeeeeeeeee4eeeeeeeeeeeeeeeeeee);
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(1, "scene-1", 1_000, vec![], internal_scene_id)],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(
            state
                .set_scene_scope_faders_enabled(internal_scene_id, false)
                .unwrap()
        );
        assert!(
            !state
                .set_scene_scope_faders_enabled(internal_scene_id, false)
                .unwrap()
        );
        assert!(!state.scene_configs[0].scope_toggles.faders);
    }

    #[test]
    fn scene_scope_pan_toggle_mutation_reports_noop() {
        let internal_scene_id = test_uuid(0xffffffffffff4fffffffffffffffffff);
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(1, "scene-1", 1_000, vec![], internal_scene_id)],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(
            state
                .set_scene_scope_pan_enabled(internal_scene_id, true)
                .unwrap()
        );
        assert!(
            !state
                .set_scene_scope_pan_enabled(internal_scene_id, true)
                .unwrap()
        );
        assert!(state.scene_configs[0].scope_toggles.pan);
    }

    #[test]
    fn scene_scope_pan_toggle_requires_existing_scene_config() {
        let mut state = ShowState::default();

        let err = state
            .set_scene_scope_pan_enabled(test_uuid(0x12121212121242128212121212121212), false)
            .unwrap_err();

        assert_eq!(err, "Scene config not found");
    }

    #[test]
    fn scene_scope_fader_toggle_requires_existing_scene_config() {
        let mut state = ShowState::default();

        let err = state
            .set_scene_scope_faders_enabled(test_uuid(0x34343434343444348434343434343434), false)
            .unwrap_err();

        assert_eq!(err, "Scene config not found");
    }

    #[test]
    fn scene_duration_allows_zero_for_immediate_movement() {
        let internal_scene_id = test_uuid(0xabababababab4abababababababababa);
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(1, "scene-1", 1_000, vec![], internal_scene_id)],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(state.set_scene_duration_ms(internal_scene_id, 0).unwrap());
        assert_eq!(state.scene_configs[0].duration_ms, 0);
    }

    #[test]
    fn link_unlinked_config_to_empty_lv1_scene_preserves_fade_data() {
        let source_id = test_uuid(0x11111111111141118111111111111111);
        let target_scene = crate::lv1::SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        };
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![SceneConfig {
                internal_scene_id: source_id,
                scene_index: None,
                scene_name: "Old Verse".to_string(),
                duration_ms: 4_200,
                channel_configs: vec![ChannelConfig {
                    group: 0,
                    channel: 1,
                    fader_db: Some(-6.5),
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
                scoped_channels: vec![crate::scenes::ChannelRef {
                    group: 0,
                    channel: 1,
                }],
                scope_toggles: SceneScopeToggles::default(),
            }],
            ..Default::default()
        };

        let changed = state
            .link_scene_config(source_id, &target_scene, false)
            .unwrap();

        assert!(changed);
        assert_eq!(state.scene_configs[0].internal_scene_id, source_id);
        assert_eq!(state.scene_configs[0].scene_index, Some(2));
        assert_eq!(state.scene_configs[0].scene_name, "Verse");
        assert_eq!(state.scene_configs[0].duration_ms, 4_200);
        assert_eq!(
            state.scene_configs[0].channel_configs[0].fader_db,
            Some(-6.5)
        );
    }

    #[test]
    fn link_requires_overwrite_when_target_has_existing_config() {
        let source_id = test_uuid(0x22222222222242228222222222222222);
        let target_id = test_uuid(0x33333333333343338333333333333333);
        let target_scene = crate::lv1::SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        };
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                SceneConfig {
                    internal_scene_id: target_id,
                    scene_index: Some(2),
                    scene_name: "Verse".to_string(),
                    duration_ms: 1_000,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: SceneScopeToggles::default(),
                },
                SceneConfig {
                    internal_scene_id: source_id,
                    scene_index: None,
                    scene_name: "Old Verse".to_string(),
                    duration_ms: 4_200,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: SceneScopeToggles::default(),
                },
            ],
            ..Default::default()
        };

        let err = state
            .link_scene_config(source_id, &target_scene, false)
            .unwrap_err();

        assert_eq!(err, "Link blocked: target scene already has a config");
        assert_eq!(state.scene_configs.len(), 2);
    }

    #[test]
    fn link_with_overwrite_deletes_existing_target_and_links_source() {
        let source_id = test_uuid(0x44444444444444448444444444444444);
        let target_id = test_uuid(0x55555555555545558555555555555555);
        let target_scene = crate::lv1::SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        };
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                SceneConfig {
                    internal_scene_id: target_id,
                    scene_index: Some(2),
                    scene_name: "Verse".to_string(),
                    duration_ms: 1_000,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: SceneScopeToggles::default(),
                },
                SceneConfig {
                    internal_scene_id: source_id,
                    scene_index: None,
                    scene_name: "Old Verse".to_string(),
                    duration_ms: 4_200,
                    channel_configs: vec![ChannelConfig {
                        group: 0,
                        channel: 1,
                        fader_db: Some(-6.5),
                        pan: None,
                        balance: None,
                        width: None,
                        pan_mode: None,
                    }],
                    scoped_channels: vec![crate::scenes::ChannelRef {
                        group: 0,
                        channel: 1,
                    }],
                    scope_toggles: SceneScopeToggles::default(),
                },
            ],
            ..Default::default()
        };

        let changed = state
            .link_scene_config(source_id, &target_scene, true)
            .unwrap();

        assert!(changed);
        assert_eq!(state.scene_configs.len(), 1);
        assert_eq!(state.scene_configs[0].internal_scene_id, source_id);
        assert_eq!(state.scene_configs[0].scene_index, Some(2));
        assert_eq!(state.scene_configs[0].scene_name, "Verse");
    }

    #[test]
    fn link_with_overwrite_keeps_linked_configs_sorted_by_scene_index() {
        let replaced_id = test_uuid(0x10101010101040108101010101010101);
        let second_id = test_uuid(0x20202020202040208202020202020202);
        let source_id = test_uuid(0x30303030303040308303030303030303);
        let target_scene = crate::lv1::SceneListEntry {
            index: 0,
            name: "Smoke A".to_string(),
        };
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                scene_config(0, "Smoke A", 1_000, Vec::new(), replaced_id),
                scene_config(1, "Smoke B", 1_000, Vec::new(), second_id),
                SceneConfig {
                    internal_scene_id: source_id,
                    scene_index: None,
                    scene_name: "Missing Smoke A".to_string(),
                    duration_ms: 4_200,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: SceneScopeToggles::default(),
                },
            ],
            ..Default::default()
        };

        state
            .link_scene_config(source_id, &target_scene, true)
            .unwrap();

        assert_eq!(state.scene_configs[0].internal_scene_id, source_id);
        assert_eq!(state.scene_configs[0].scene_index, Some(0));
        assert_eq!(state.scene_configs[1].internal_scene_id, second_id);
        assert_eq!(state.scene_configs[1].scene_index, Some(1));
    }

    #[test]
    fn link_with_overwrite_clears_selected_and_cued_when_replacing_target() {
        let source_id = test_uuid(0x77777777777747778777777777777777);
        let target_id = test_uuid(0x88888888888848888888888888888888);
        let target_scene = crate::lv1::SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        };
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![
                SceneConfig {
                    internal_scene_id: target_id,
                    scene_index: Some(2),
                    scene_name: "Verse".to_string(),
                    duration_ms: 1_000,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: SceneScopeToggles::default(),
                },
                SceneConfig {
                    internal_scene_id: source_id,
                    scene_index: None,
                    scene_name: "Old Verse".to_string(),
                    duration_ms: 4_200,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: SceneScopeToggles::default(),
                },
            ],
            cued_scene_internal_id: Some(target_id),
            selected_scene_internal_id: Some(target_id.to_string()),
            ..Default::default()
        };

        let changed = state
            .link_scene_config(source_id, &target_scene, true)
            .unwrap();

        assert!(changed);
        assert_eq!(state.selected_scene_internal_id, None);
        assert_eq!(state.cued_scene_internal_id, None);
    }

    #[test]
    fn link_rejects_already_linked_source_config() {
        let source_id = test_uuid(0x99999999999949998999999999999999);
        let target_scene = crate::lv1::SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        };
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![SceneConfig {
                internal_scene_id: source_id,
                scene_index: Some(1),
                scene_name: "Intro".to_string(),
                duration_ms: 1_000,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: SceneScopeToggles::default(),
            }],
            ..Default::default()
        };

        let err = state
            .link_scene_config(source_id, &target_scene, false)
            .unwrap_err();

        assert_eq!(err, "Link blocked: source scene is already linked");
    }

    #[test]
    fn delete_scene_config_removes_config_and_clears_selected_and_cued() {
        let internal_scene_id = test_uuid(0x66666666666646668666666666666666);
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(1, "scene-1", 1_000, vec![], internal_scene_id)],
            cued_scene_internal_id: Some(internal_scene_id),
            selected_scene_internal_id: Some(internal_scene_id.to_string()),
            ..Default::default()
        };

        let changed = state.delete_scene_config(internal_scene_id).unwrap();

        assert!(changed);
        assert!(state.scene_configs.is_empty());
        assert_eq!(state.cued_scene_internal_id, None);
        assert_eq!(state.selected_scene_internal_id, None);
    }
}

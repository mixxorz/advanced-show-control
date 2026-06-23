use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};
use crate::lv1::Lv1StateSnapshot;
use crate::show::show_file::{ShowFile, export_show_file};

use super::types::{SceneConfig, ShowDocument};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShowState {
    lockout: bool,
    scene_configs: Vec<SceneConfig>,
    cued_scene_internal_id: Option<Uuid>,
    selected_scene_id: Option<String>,
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
            self.scene_configs = crate::show::scene_alignment::align_scene_configs(
                self.scene_configs.clone(),
                &lv1.scene_list,
            );
        }
        self.selected_scene_id = self
            .scene_configs
            .first()
            .map(|scene| scene.internal_scene_id.to_string());
        self.show_file_path = None;
        self.show_file_dirty = false;
        self.show_file_last_saved_at = None;
        self.selected_scene_id.clone()
    }

    pub(crate) fn mark_saved(&mut self, path: std::path::PathBuf, saved_at: String) {
        self.show_file_path = Some(path);
        self.show_file_last_saved_at = Some(saved_at);
        self.show_file_dirty = false;
    }

    pub(crate) fn set_selected_scene_id(&mut self, selected_scene_id: Option<String>) -> bool {
        if self.selected_scene_id == selected_scene_id {
            return false;
        }
        self.selected_scene_id = selected_scene_id;
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
        export_show_file(self.snapshot(), saved_at)
    }

    pub(crate) fn scene_configs_mut(&mut self) -> &mut Vec<SceneConfig> {
        &mut self.scene_configs
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
            cued_scene_id: self.cued_scene_internal_id.map(|id| id.to_string()),
            selected_scene_id: self.selected_scene_id.clone(),
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

    pub(crate) fn get_scene_config_mut(
        &mut self,
        internal_scene_id: Uuid,
    ) -> Option<&mut SceneConfig> {
        self.scene_configs
            .iter_mut()
            .find(|scene| scene.internal_scene_id == internal_scene_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::{ChannelInfo, PanMode};
    use crate::show::{ChannelConfig, SceneScopeToggles};
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

        let scene_id = state.scene_configs[0].internal_scene_id;

        assert!(state.set_channel_scoped(scene_id, 0, 1, true).unwrap());
        assert!(!state.set_channel_scoped(scene_id, 0, 1, true).unwrap());
    }

    #[test]
    fn store_scene_config_snapshots_current_channels_and_scopes() {
        let scene_id = test_uuid(0x11111111111141118111111111111111);
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(1, "scene-1", 1_000, vec![], scene_id)],
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

        assert!(state.store_scene_config(scene_id, &channels).unwrap());

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
        let scene_id = test_uuid(0xabcabcabcabc4abc8abcabcabcabcabc);

        let err = state.store_scene_config(scene_id, &channels).unwrap_err();

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

        let scene_id = state.scene_configs[0].internal_scene_id;

        assert!(
            state
                .store_scene_config(scene_id, &[channel(0, 1, "Lead", -6.0)])
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

        let scene_id = state.scene_configs[0].internal_scene_id;

        assert!(
            state
                .store_scene_config(
                    scene_id,
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

        let scene_id = state.scene_configs[0].internal_scene_id;

        assert!(
            state
                .store_scene_config(
                    scene_id,
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
        let scene_id = test_uuid(0xdadadadadada4ada8adadadadadadada);
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(1, "scene-1", 1_000, vec![], scene_id)],
            cued_scene_internal_id: None,
            ..Default::default()
        };
        let changed = state
            .store_scene_config(scene_id, &[channel(0, 1, "Lead", -6.0)])
            .unwrap();

        assert!(changed);
        assert!(state.scene_configs[0].scope_toggles.faders);
    }

    #[test]
    fn store_scene_config_preserves_fader_scope_toggle() {
        let scene_id = test_uuid(0xdddddddddddd4ddddddddddddddddddd);
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(1, "scene-1", 1_000, vec![], scene_id)],
            cued_scene_internal_id: None,
            ..Default::default()
        };
        assert!(
            state
                .set_scene_scope_faders_enabled(scene_id, false)
                .unwrap()
        );

        state
            .store_scene_config(scene_id, &[channel(0, 1, "Lead", -3.0)])
            .unwrap();

        assert!(!state.scene_configs[0].scope_toggles.faders);
    }

    #[test]
    fn scene_scope_fader_toggle_mutation_reports_noop() {
        let scene_id = test_uuid(0xeeeeeeeeeeee4eeeeeeeeeeeeeeeeeee);
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(1, "scene-1", 1_000, vec![], scene_id)],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(
            state
                .set_scene_scope_faders_enabled(scene_id, false)
                .unwrap()
        );
        assert!(
            !state
                .set_scene_scope_faders_enabled(scene_id, false)
                .unwrap()
        );
        assert!(!state.scene_configs[0].scope_toggles.faders);
    }

    #[test]
    fn scene_scope_pan_toggle_mutation_reports_noop() {
        let scene_id = test_uuid(0xffffffffffff4fffffffffffffffffff);
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(1, "scene-1", 1_000, vec![], scene_id)],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(state.set_scene_scope_pan_enabled(scene_id, true).unwrap());
        assert!(!state.set_scene_scope_pan_enabled(scene_id, true).unwrap());
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
        let scene_id = test_uuid(0xabababababab4abababababababababa);
        let mut state = ShowState {
            lockout: false,
            scene_configs: vec![scene_config(1, "scene-1", 1_000, vec![], scene_id)],
            cued_scene_internal_id: None,
            ..Default::default()
        };

        assert!(state.set_scene_duration_ms(scene_id, 0).unwrap());
        assert_eq!(state.scene_configs[0].duration_ms, 0);
    }
}

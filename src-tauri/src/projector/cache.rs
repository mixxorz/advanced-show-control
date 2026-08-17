use std::collections::VecDeque;
use std::path::PathBuf;

use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity};
use crate::cue_lists::CueListsProjectionState;
use crate::fade::FadeEvent;
use crate::logging::UiLogEvent;
use crate::lv1::Lv1Event;
use crate::projector::{
    AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, SceneSummary,
};
use crate::scenes::ScenesProjectionState;
use crate::settings::AppSettings;
use crate::show::ShowProjectionState;

pub const MAX_PROJECTOR_LOGS: usize = 200;

#[derive(Debug)]
struct Lv1Projection {
    connection: AppConnectionState,
    current_scene: Option<SceneSummary>,
    scenes: Vec<SceneSummary>,
    channels: Vec<ChannelSummary>,
}

#[derive(Debug)]
pub struct ProjectionCache {
    active_generation: u64,
    state_version: u64,
    lv1_projection: Option<Lv1Projection>,
    discovered_lv1_systems: Vec<DiscoveredLv1System>,
    connected_lv1_identity: Option<Lv1SystemIdentity>,
    fade_state: AppFadeState,
    selected_scene_internal_id: Option<String>,
    lockout: bool,
    scene_configs: Vec<crate::scenes::SceneConfig>,
    scene_settings_clipboard_available: bool,
    cue_lists: Vec<crate::cue_lists::CueList>,
    active_cue_list_id: Option<String>,
    cued_cue_entry_id: Option<String>,
    last_cue_recall_status: Option<String>,
    show_file_path: Option<PathBuf>,
    show_file_dirty: bool,
    show_file_last_saved_at: Option<String>,
    settings: AppSettings,
    logs: VecDeque<AppLogEntry>,
    next_log_id: u64,
    last_event_at: Option<String>,
}

impl Default for Lv1Projection {
    fn default() -> Self {
        Self {
            connection: AppConnectionState::Disconnected,
            current_scene: None,
            scenes: Vec::new(),
            channels: Vec::new(),
        }
    }
}

impl Default for ProjectionCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectionCache {
    pub fn new() -> Self {
        Self {
            active_generation: 0,
            state_version: 0,
            lv1_projection: None,
            discovered_lv1_systems: Vec::new(),
            connected_lv1_identity: None,
            fade_state: AppFadeState::Idle,
            selected_scene_internal_id: None,
            lockout: false,
            scene_configs: Vec::new(),
            scene_settings_clipboard_available: false,
            cue_lists: Vec::new(),
            active_cue_list_id: None,
            cued_cue_entry_id: None,
            last_cue_recall_status: None,
            settings: AppSettings::default(),
            show_file_path: None,
            show_file_dirty: false,
            show_file_last_saved_at: None,
            logs: VecDeque::new(),
            next_log_id: 1,
            last_event_at: None,
        }
    }

    pub fn apply_settings(&mut self, settings: AppSettings) {
        self.settings = settings;
    }

    pub fn set_active_generation(&mut self, generation: u64) {
        self.active_generation = generation;
    }

    pub fn reset_for_generation(&mut self, generation: u64) {
        self.active_generation = generation;
        self.lv1_projection = None;
        self.fade_state = AppFadeState::Idle;
    }

    pub fn is_active_generation(&self, generation: u64) -> bool {
        self.active_generation == generation
    }

    pub fn apply_show_state(&mut self, state: ShowProjectionState) {
        self.lockout = state.lockout;
        self.show_file_path = state.show_file_path;
        self.show_file_dirty = state.show_file_dirty;
        self.show_file_last_saved_at = state.show_file_last_saved_at;
        self.discovered_lv1_systems = state.discovered_lv1_systems;
        self.connected_lv1_identity = state.connected_lv1_identity;
        self.last_event_at = state.last_event_at;
    }

    pub fn apply_scenes_state(&mut self, state: ScenesProjectionState) {
        self.scene_configs = state.scene_configs;
        self.selected_scene_internal_id = state.selected_scene_internal_id;
        self.scene_settings_clipboard_available = state.scene_settings_clipboard_available;
    }

    pub fn apply_cue_lists_state(&mut self, state: CueListsProjectionState) {
        self.cue_lists = state.document.cue_lists;
        self.active_cue_list_id = state.document.active_cue_list_id.map(|id| id.to_string());
        self.cued_cue_entry_id = state.document.cued_cue_entry_id.map(|id| id.to_string());
        self.last_cue_recall_status = state.last_recall_status;
    }

    pub fn apply_lv1_snapshot(&mut self, generation: u64, snapshot: crate::lv1::Lv1StateSnapshot) {
        if generation != self.active_generation {
            return;
        }
        let projection = self.ensure_lv1_projection();
        projection.connection = match snapshot.connection {
            crate::lv1::ConnectionStatus::Connected => AppConnectionState::Connected,
            crate::lv1::ConnectionStatus::Connecting => AppConnectionState::Connecting,
            crate::lv1::ConnectionStatus::Disconnected => AppConnectionState::Disconnected,
        };
        projection.current_scene = snapshot.scene.map(|scene| SceneSummary {
            index: scene.index,
            name: scene.name,
        });
        projection.scenes = snapshot
            .scene_list
            .into_iter()
            .map(|scene| SceneSummary {
                index: scene.index,
                name: scene.name,
            })
            .collect();
        projection.channels = snapshot
            .channels
            .into_iter()
            .map(|channel| ChannelSummary {
                group: channel.group,
                channel: channel.channel,
                name: channel.name,
            })
            .collect();
    }

    pub fn apply_lv1_event(&mut self, generation: u64, event: &Lv1Event) -> bool {
        if generation != self.active_generation {
            return false;
        }
        match event {
            Lv1Event::Connected => {
                self.ensure_lv1_projection().connection = AppConnectionState::Connected
            }
            Lv1Event::Disconnected { .. } => self.lv1_projection = None,
            Lv1Event::PingReceived { .. }
            | Lv1Event::FaderChanged { .. }
            | Lv1Event::MuteChanged { .. }
            | Lv1Event::PanChanged { .. }
            | Lv1Event::BalanceChanged { .. }
            | Lv1Event::WidthChanged { .. } => return false,
            Lv1Event::SceneChanged(crate::lv1::SceneObservation { scene, .. }) => {
                self.ensure_lv1_projection().current_scene = Some(SceneSummary {
                    index: scene.index,
                    name: scene.name.clone(),
                });
            }
            Lv1Event::SceneListChanged(scene_list) => {
                self.ensure_lv1_projection().scenes = scene_list
                    .iter()
                    .map(|scene| SceneSummary {
                        index: scene.index,
                        name: scene.name.clone(),
                    })
                    .collect();
            }
            Lv1Event::ChannelTopologyChanged(channels) => {
                self.ensure_lv1_projection().channels = channels
                    .iter()
                    .map(|channel| ChannelSummary {
                        group: channel.group,
                        channel: channel.channel,
                        name: channel.name.clone(),
                    })
                    .collect();
            }
        }
        true
    }

    pub fn apply_fade_event(&mut self, generation: u64, event: &FadeEvent) -> bool {
        if generation != self.active_generation {
            return false;
        }
        match event {
            FadeEvent::FadeStarted => self.fade_state = AppFadeState::Running,
            FadeEvent::FadeCompleted | FadeEvent::FadeAborted => {
                self.fade_state = AppFadeState::Idle
            }
            FadeEvent::ChannelCompleted { .. } | FadeEvent::ChannelCancelled { .. } => {}
            FadeEvent::ChannelOverride { .. } => self.fade_state = AppFadeState::Blocked,
            FadeEvent::WriteFailed { .. } => {}
        }
        true
    }

    pub fn append_log(&mut self, event: UiLogEvent) {
        let entry = AppLogEntry {
            id: self.next_log_id,
            timestamp: crate::time::current_timestamp_millis(),
            severity: event.severity,
            message: event.message,
        };
        self.next_log_id = self.next_log_id.saturating_add(1);
        self.logs.push_back(entry);
        while self.logs.len() > MAX_PROJECTOR_LOGS {
            self.logs.pop_front();
        }
    }

    pub fn build_snapshot(&mut self) -> AppViewState {
        self.state_version = self.state_version.saturating_add(1);
        let state_version = self.state_version;

        let (connection, current_scene, scenes, channels) = self
            .lv1_projection
            .as_ref()
            .map(|projection| {
                (
                    projection.connection.clone(),
                    projection.current_scene.clone(),
                    projection.scenes.clone(),
                    projection.channels.clone(),
                )
            })
            .unwrap_or((
                AppConnectionState::Disconnected,
                None,
                Vec::new(),
                Vec::new(),
            ));

        AppViewState {
            connection,
            discovered_lv1_systems: self.discovered_lv1_systems.clone(),
            connected_lv1_identity: self.connected_lv1_identity.clone(),
            current_scene,
            scenes: scenes.clone(),
            scene_count: scenes.len(),
            channel_count: channels.len(),
            channels,
            fade_state: self.fade_state.clone(),
            lockout: self.lockout,
            scene_configs: self.scene_configs.clone(),
            scene_settings_clipboard_available: self.scene_settings_clipboard_available,
            cue_lists: self.cue_lists.clone(),
            active_cue_list_id: self.active_cue_list_id.clone(),
            cued_cue_entry_id: self.cued_cue_entry_id.clone(),
            last_cue_recall_status: self.last_cue_recall_status.clone(),
            selected_scene_internal_id: self.selected_scene_internal_id.clone(),
            show_file_name: self
                .show_file_path
                .as_ref()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
                .unwrap_or_else(|| "Untitled Session".to_string()),
            show_file_path: self
                .show_file_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            show_file_dirty: self.show_file_dirty,
            show_file_last_saved_at: self.show_file_last_saved_at.clone(),
            settings: self.settings.clone(),
            logs: self.logs.iter().cloned().collect(),
            last_event_at: self.last_event_at.clone(),
            state_version,
        }
    }

    fn ensure_lv1_projection(&mut self) -> &mut Lv1Projection {
        self.lv1_projection
            .get_or_insert_with(Lv1Projection::default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::FadeParameter;
    use crate::lv1::{ChannelInfo, SceneState};
    use crate::projector::LogSeverity;
    use crate::settings::AppSettings;
    #[test]
    fn cache_builds_initial_disconnected_snapshot_with_incrementing_versions() {
        let mut cache = ProjectionCache::new();

        let first = cache.build_snapshot();
        let second = cache.build_snapshot();

        assert_eq!(first.connection, AppConnectionState::Disconnected);
        assert_eq!(first.show_file_name, "Untitled Session");
        assert_eq!(first.state_version, 1);
        assert_eq!(second.state_version, 2);
    }

    #[test]
    fn cache_applies_lv1_scene_and_topology_events() {
        let mut cache = ProjectionCache::new();

        cache.apply_lv1_event(0, &Lv1Event::Connected);
        cache.apply_lv1_event(
            0,
            &Lv1Event::SceneChanged(crate::lv1::SceneObservation {
                sequence: 1,
                scene: SceneState {
                    index: 3,
                    name: "Bridge".to_string(),
                },
            }),
        );
        cache.apply_lv1_event(
            0,
            &Lv1Event::ChannelTopologyChanged(vec![ChannelInfo {
                group: 1,
                channel: 2,
                name: "Vox".to_string(),
                gain_db: -5.0,
                muted: false,
                pan: Some(0.0),
                balance: None,
                width: None,
                pan_mode: None,
            }]),
        );

        let snapshot = cache.build_snapshot();

        assert_eq!(snapshot.connection, AppConnectionState::Connected);
        assert_eq!(snapshot.current_scene.unwrap().name, "Bridge");
        assert_eq!(snapshot.channel_count, 1);
        assert_eq!(snapshot.channels[0].name, "Vox");
    }

    #[test]
    fn cache_resets_generation_scoped_state_but_preserves_app_lifetime_state() {
        let mut cache = ProjectionCache::new();
        cache.apply_show_state(ShowProjectionState {
            lockout: true,
            show_file_path: Some(PathBuf::from("show.asc")),
            show_file_name: "show.asc".to_string(),
            show_file_dirty: true,
            show_file_last_saved_at: None,
            discovered_lv1_systems: Vec::new(),
            connected_lv1_identity: None,
            last_event_at: None,
        });
        cache.apply_settings(AppSettings {
            auto_save_sessions: true,
            ..Default::default()
        });
        cache.apply_scenes_state(ScenesProjectionState {
            scene_configs: vec![crate::scenes::SceneConfig {
                internal_scene_id: uuid::Uuid::from_u128(1),
                scene_index: Some(1),
                scene_name: "A".to_string(),
                duration_ms: 1_000,
                channel_configs: vec![],
                scoped_channels: vec![],
                scope_toggles: Default::default(),
            }],
            selected_scene_internal_id: Some("scene-config".to_string()),
            scene_settings_clipboard_available: true,
            ready_generation: Some(0),
        });
        let cue_list_id = uuid::Uuid::from_u128(2);
        cache.apply_cue_lists_state(CueListsProjectionState {
            document: crate::cue_lists::CueListDocument {
                cue_lists: vec![crate::cue_lists::CueList {
                    id: cue_list_id,
                    name: "Main".to_string(),
                    entries: vec![],
                }],
                active_cue_list_id: Some(cue_list_id),
                cued_cue_entry_id: None,
            },
            last_recall_status: Some("recalled".to_string()),
        });
        cache.append_log(UiLogEvent {
            severity: LogSeverity::Info,
            message: "kept".to_string(),
        });
        cache.apply_lv1_event(0, &Lv1Event::Connected);
        cache.apply_lv1_event(
            0,
            &Lv1Event::SceneChanged(crate::lv1::SceneObservation {
                sequence: 1,
                scene: SceneState {
                    index: 1,
                    name: "A".to_string(),
                },
            }),
        );
        cache.apply_lv1_event(
            0,
            &Lv1Event::SceneListChanged(vec![crate::lv1::SceneListEntry {
                index: 1,
                name: "A".to_string(),
            }]),
        );
        cache.apply_lv1_event(
            0,
            &Lv1Event::ChannelTopologyChanged(vec![ChannelInfo {
                group: 1,
                channel: 1,
                name: "Vox".to_string(),
                gain_db: -5.0,
                muted: true,
                pan: Some(0.5),
                balance: None,
                width: None,
                pan_mode: None,
            }]),
        );
        cache.apply_fade_event(0, &FadeEvent::FadeStarted);
        let version_a = cache.build_snapshot().state_version;

        cache.reset_for_generation(1);
        cache.apply_lv1_event(1, &Lv1Event::Connected);
        let snapshot = cache.build_snapshot();

        assert!(snapshot.current_scene.is_none());
        assert!(snapshot.scenes.is_empty());
        assert!(snapshot.channels.is_empty());
        assert_eq!(snapshot.fade_state, AppFadeState::Idle);
        assert!(snapshot.lockout);
        assert_eq!(snapshot.show_file_name, "show.asc");
        assert!(snapshot.settings.auto_save_sessions);
        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(snapshot.cue_lists[0].name, "Main");
        assert_eq!(
            snapshot.selected_scene_internal_id.as_deref(),
            Some("scene-config")
        );
        assert_eq!(snapshot.last_cue_recall_status.as_deref(), Some("recalled"));
        assert_eq!(snapshot.logs[0].message, "kept");
        assert!(snapshot.state_version > version_a);
    }

    #[test]
    fn parameter_only_events_do_not_change_projection() {
        let mut cache = ProjectionCache::new();
        cache.apply_lv1_event(0, &Lv1Event::Connected);
        let before = cache.build_snapshot();
        for event in [
            Lv1Event::FaderChanged {
                group: 1,
                channel: 1,
                gain_db: -1.0,
            },
            Lv1Event::MuteChanged {
                group: 1,
                channel: 1,
                muted: true,
            },
            Lv1Event::PanChanged {
                group: 1,
                channel: 1,
                pan: 0.5,
            },
            Lv1Event::BalanceChanged {
                group: 1,
                channel: 1,
                balance: 0.5,
            },
            Lv1Event::WidthChanged {
                group: 1,
                channel: 1,
                width: 0.5,
            },
            Lv1Event::PingReceived { sequence: 1 },
        ] {
            assert!(!cache.apply_lv1_event(0, &event));
        }
        let after = cache.build_snapshot();
        assert_eq!(before.connection, after.connection);
        assert!(after.channels.is_empty());
    }

    #[test]
    fn cache_clears_lv1_snapshot_on_disconnect() {
        let mut cache = ProjectionCache::new();

        cache.connected_lv1_identity = Some(Lv1SystemIdentity {
            uuid: Some("connected-uuid".to_string()),
            host: Some("lv1.local".to_string()),
            address: "192.0.2.10".to_string(),
            port: 7788,
        });

        cache.apply_lv1_event(
            0,
            &Lv1Event::Disconnected {
                reason: "link lost".to_string(),
            },
        );

        let snapshot = cache.build_snapshot();

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert!(snapshot.current_scene.is_none());
        assert_eq!(snapshot.scenes.len(), 0);
    }

    #[test]
    fn lv1_disconnect_does_not_clear_show_owned_connection_metadata() {
        let mut cache = ProjectionCache::new();

        let connected_identity = Lv1SystemIdentity {
            uuid: Some("connected-uuid".to_string()),
            host: Some("lv1.local".to_string()),
            address: "192.0.2.10".to_string(),
            port: 7788,
        };

        cache.apply_show_state(ShowProjectionState {
            lockout: false,
            show_file_path: None,
            show_file_name: "Untitled Session".to_string(),
            show_file_dirty: false,
            show_file_last_saved_at: None,
            discovered_lv1_systems: Vec::new(),
            connected_lv1_identity: Some(connected_identity.clone()),
            last_event_at: None,
        });

        let changed = cache.apply_lv1_event(
            0,
            &Lv1Event::Disconnected {
                reason: "link lost".to_string(),
            },
        );

        let snapshot = cache.build_snapshot();

        assert!(changed);
        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert_eq!(snapshot.connected_lv1_identity, Some(connected_identity));
    }

    #[test]
    fn cache_applies_scenes_projection_state_separately_from_show_state() {
        let mut cache = ProjectionCache::new();

        cache.apply_show_state(ShowProjectionState {
            lockout: true,
            show_file_path: None,
            show_file_name: "Untitled Session".to_string(),
            show_file_dirty: false,
            show_file_last_saved_at: None,
            discovered_lv1_systems: Vec::new(),
            connected_lv1_identity: None,
            last_event_at: None,
        });
        cache.apply_scenes_state(crate::scenes::ScenesProjectionState {
            scene_configs: vec![crate::scenes::SceneConfig {
                internal_scene_id: uuid::Uuid::from_u128(0x11111111111141118111111111111111),
                scene_index: Some(5),
                scene_name: "Verse".to_string(),
                duration_ms: 1500,
                channel_configs: vec![],
                scoped_channels: vec![],
                scope_toggles: Default::default(),
            }],
            selected_scene_internal_id: Some("selected-id".to_string()),
            scene_settings_clipboard_available: true,
            ready_generation: Some(0),
        });

        let snapshot = cache.build_snapshot();

        assert!(snapshot.lockout);
        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(
            snapshot.selected_scene_internal_id.as_deref(),
            Some("selected-id")
        );
        assert!(snapshot.scene_settings_clipboard_available);
    }

    #[test]
    fn cache_applies_cue_list_projection_state() {
        let mut cache = ProjectionCache::new();
        let cue_list_id = uuid::Uuid::from_u128(1);
        cache.apply_cue_lists_state(CueListsProjectionState {
            document: crate::cue_lists::CueListDocument {
                cue_lists: vec![crate::cue_lists::CueList {
                    id: cue_list_id,
                    name: "Main".to_string(),
                    entries: vec![],
                }],
                active_cue_list_id: Some(cue_list_id),
                cued_cue_entry_id: None,
            },
            last_recall_status: None,
        });

        let snapshot = cache.build_snapshot();

        assert_eq!(snapshot.cue_lists[0].name, "Main");
        assert_eq!(
            snapshot.active_cue_list_id.as_deref(),
            Some(cue_list_id.to_string().as_str())
        );
    }

    #[test]
    fn cache_applies_fade_state_events() {
        let mut cache = ProjectionCache::new();

        assert!(cache.apply_fade_event(0, &FadeEvent::FadeStarted));
        assert_eq!(cache.build_snapshot().fade_state, AppFadeState::Running);

        assert!(cache.apply_fade_event(
            0,
            &FadeEvent::ChannelOverride {
                group: 1,
                channel: 1,
                parameter: FadeParameter::FaderDb,
            }
        ));
        assert_eq!(cache.build_snapshot().fade_state, AppFadeState::Blocked);

        assert!(cache.apply_fade_event(0, &FadeEvent::FadeCompleted));
        assert_eq!(cache.build_snapshot().fade_state, AppFadeState::Idle);
    }

    #[test]
    fn cache_ignores_stale_generation_fade_events() {
        let mut cache = ProjectionCache::new();
        cache.set_active_generation(2);

        assert!(!cache.apply_fade_event(1, &FadeEvent::FadeStarted));

        assert_eq!(cache.build_snapshot().fade_state, AppFadeState::Idle);
    }

    #[test]
    fn cache_keeps_fade_state_when_channel_cancelled() {
        let mut cache = ProjectionCache::new();

        cache.apply_fade_event(0, &FadeEvent::FadeStarted);
        cache.apply_fade_event(
            0,
            &FadeEvent::ChannelCancelled {
                group: 1,
                channel: 1,
                parameter: FadeParameter::FaderDb,
            },
        );

        assert_eq!(cache.build_snapshot().fade_state, AppFadeState::Running);
    }

    #[test]
    fn cache_owns_bounded_log_entries() {
        let mut cache = ProjectionCache::new();

        for index in 0..(MAX_PROJECTOR_LOGS + 2) {
            cache.append_log(UiLogEvent {
                severity: LogSeverity::Info,
                message: format!("log {index}"),
            });
        }

        let snapshot = cache.build_snapshot();

        assert_eq!(snapshot.logs.len(), MAX_PROJECTOR_LOGS);
        assert_eq!(snapshot.logs[0].id, 3);
        assert_eq!(snapshot.logs.last().unwrap().message, "log 201");
    }
}

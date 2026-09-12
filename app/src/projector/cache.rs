use std::collections::VecDeque;

use crate::fade::FadeEvent;
use crate::logging::UiLogEvent;
use crate::lv1::Lv1Event;
use crate::projector::{
    AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, SceneSummary,
};
use crate::runtime::AppStateSnapshot;

pub const MAX_PROJECTOR_LOGS: usize = 200;

#[derive(Debug)]
struct Lv1Projection {
    connection: AppConnectionState,
    current_scene: Option<SceneSummary>,
    scenes: Vec<SceneSummary>,
    channels: Vec<ChannelSummary>,
}

/// @cc [owner:mixxorz,label:architecture;state] projector-cache-ownership
/// The cache MUST own only generation-bound LV1/Fade projection state, bounded native UI logs, and
/// snapshot/log counters; app-lifetime Show, Scenes, Cue Lists, and Settings state MUST be read from
/// `AppStateSnapshot` when a view is built rather than copied into this cache.
#[derive(Debug)]
pub struct ProjectionCache {
    active_generation: u64,
    state_version: u64,
    lv1_projection: Option<Lv1Projection>,
    fade_state: AppFadeState,
    logs: VecDeque<AppLogEntry>,
    next_log_id: u64,
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
            fade_state: AppFadeState::Idle,
            logs: VecDeque::new(),
            next_log_id: 1,
        }
    }

    pub fn set_active_generation(&mut self, generation: u64) {
        self.active_generation = generation;
    }

    /// @cc [owner:mixxorz,label:generation;state] generation-reset-boundary
    /// A generation reset MUST clear all LV1-derived projection data and return Fade to `Idle`, while
    /// preserving logs and snapshot/log counters that remain valid across connections.
    pub fn reset_for_generation(&mut self, generation: u64) {
        self.active_generation = generation;
        self.reset_generation_scoped_state();
    }

    pub fn reset_generation_scoped_state(&mut self) {
        self.lv1_projection = None;
        self.fade_state = AppFadeState::Idle;
    }

    pub fn is_active_generation(&self, generation: u64) -> bool {
        self.active_generation == generation
    }

    pub fn active_generation(&self) -> u64 {
        self.active_generation
    }

    /// @cc [owner:mixxorz,label:generation;safety] authoritative-snapshot-generation-filter
    /// An authoritative LV1 snapshot MUST replace live projection fields only when its generation
    /// equals the cache's active generation; a stale snapshot MUST leave the cache unchanged.
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

    /// @cc [owner:mixxorz,label:generation;projection] lv1-event-materiality
    /// Stale-generation LV1 facts and parameter/keepalive facts MUST leave projected state unchanged
    /// and return `false`; accepted connection, scene, scene-list, topology, and disconnect facts MUST
    /// update or clear the live projection and return `true` so emission dirtiness tracks UI materiality.
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

    /// @cc [owner:mixxorz,label:generation;projection] fade-event-generation-filter
    /// Fade facts from a stale generation MUST neither mutate projected Fade state nor mark the view
    /// dirty; accepted-generation Fade facts MUST preserve the aggregate Running/Blocked/Idle mapping
    /// and report projector activity even when a per-channel fact leaves that aggregate unchanged.
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

    /// @cc [owner:mixxorz,label:logging;reliability] bounded-ordered-ui-logs
    /// Accepted UI log events MUST remain in arrival order and evict the oldest entries until no more
    /// than `MAX_PROJECTOR_LOGS` remain. Cache-local IDs start at 1 and increase with saturation, so
    /// they MAY repeat at `u64::MAX`.
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

    /**
     * @cc [owner:mixxorz,label:projection;consistency] retained-state-at-build
     * Every snapshot MUST combine the cache's current generation-bound state and logs with the
     * supplied retained app-state snapshot; missing LV1 state MUST project disconnected with no live
     * scene, scene-list, or channel data rather than removing app-lifetime state.
     */
    /**
     * @cc [owner:mixxorz,label:projection;ordering] snapshot-version-monotonic
     * Each build MUST increment the cache-local `state_version` until it saturates at `u64::MAX`, and
     * generation resets MUST NOT reset it. Versions are therefore nondecreasing, not indefinitely
     * strictly increasing.
     */
    pub fn build_snapshot(&mut self, state: &AppStateSnapshot) -> AppViewState {
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
            discovered_lv1_systems: state.show.discovered_lv1_systems.clone(),
            connected_lv1_identity: state.show.connected_lv1_identity.clone(),
            current_scene,
            scenes: scenes.clone(),
            scene_count: scenes.len(),
            channel_count: channels.len(),
            channels,
            fade_state: self.fade_state.clone(),
            lockout: state.show.lockout,
            scene_configs: state.scenes.scene_configs.clone(),
            scene_settings_clipboard_available: state.scenes.scene_settings_clipboard_available,
            cue_lists: state.cue_lists.document.cue_lists.clone(),
            active_cue_list_id: state
                .cue_lists
                .document
                .active_cue_list_id
                .map(|id| id.to_string()),
            cued_cue_entry_id: state
                .cue_lists
                .document
                .cued_cue_entry_id
                .map(|id| id.to_string()),
            last_cue_recall_status: state.cue_lists.last_recall_status.clone(),
            selected_scene_internal_id: state.scenes.selected_scene_internal_id.clone(),
            show_file_name: state.show.show_file_name.clone(),
            show_file_path: state
                .show
                .show_file_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            show_file_dirty: state.show.show_file_dirty,
            show_file_last_saved_at: state.show.show_file_last_saved_at.clone(),
            settings: state.settings.clone(),
            logs: self.logs.iter().cloned().collect(),
            last_event_at: state.show.last_event_at.clone(),
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
    use crate::connection_state::Lv1SystemIdentity;
    use crate::cue_lists::CueListsProjectionState;
    use crate::fade::FadeParameter;
    use crate::lv1::{ChannelInfo, SceneState};
    use crate::projector::LogSeverity;
    use crate::scenes::ScenesProjectionState;
    use crate::settings::AppSettings;
    use crate::show::ShowProjectionState;
    use std::path::PathBuf;
    #[test]
    fn cache_builds_initial_disconnected_snapshot_with_incrementing_versions() {
        let mut cache = ProjectionCache::new();
        let state = AppStateSnapshot::default();

        let first = cache.build_snapshot(&state);
        let second = cache.build_snapshot(&state);

        assert_eq!(first.connection, AppConnectionState::Disconnected);
        assert_eq!(first.show_file_name, "Untitled Session");
        assert_eq!(first.state_version, 1);
        assert_eq!(second.state_version, 2);
    }

    #[test]
    fn cache_applies_lv1_scene_and_topology_events() {
        let mut cache = ProjectionCache::new();
        let state = AppStateSnapshot::default();

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

        let snapshot = cache.build_snapshot(&state);

        assert_eq!(snapshot.connection, AppConnectionState::Connected);
        assert_eq!(snapshot.current_scene.unwrap().name, "Bridge");
        assert_eq!(snapshot.channel_count, 1);
        assert_eq!(snapshot.channels[0].name, "Vox");
    }

    #[test]
    fn cache_resets_generation_scoped_state_but_preserves_app_lifetime_state() {
        let mut cache = ProjectionCache::new();
        let mut state = AppStateSnapshot {
            show: ShowProjectionState {
                lockout: true,
                show_file_path: Some(PathBuf::from("show.asc")),
                show_file_name: "show.asc".to_string(),
                show_file_dirty: true,
                show_file_last_saved_at: None,
                discovered_lv1_systems: Vec::new(),
                connected_lv1_identity: None,
                last_event_at: None,
            },
            settings: AppSettings {
                auto_save_sessions: true,
                ..Default::default()
            },
            scenes: ScenesProjectionState {
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
            },
            ..Default::default()
        };
        let cue_list_id = uuid::Uuid::from_u128(2);
        state.cue_lists = CueListsProjectionState {
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
        };
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
        let version_a = cache.build_snapshot(&state).state_version;

        cache.reset_for_generation(1);
        cache.apply_lv1_event(1, &Lv1Event::Connected);
        let snapshot = cache.build_snapshot(&state);

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
        let state = AppStateSnapshot::default();
        cache.apply_lv1_event(0, &Lv1Event::Connected);
        let before = cache.build_snapshot(&state);
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
        let after = cache.build_snapshot(&state);
        assert_eq!(before.connection, after.connection);
        assert!(after.channels.is_empty());
    }

    #[test]
    fn lv1_disconnect_clears_live_state_but_preserves_show_owned_metadata() {
        let mut cache = ProjectionCache::new();
        let mut state = AppStateSnapshot::default();
        cache.apply_lv1_snapshot(
            0,
            crate::lv1::Lv1StateSnapshot {
                connection: crate::lv1::ConnectionStatus::Connected,
                scene: Some(SceneState {
                    index: 3,
                    name: "Bridge".into(),
                }),
                scene_list: vec![crate::lv1::SceneListEntry {
                    index: 3,
                    name: "Bridge".into(),
                }],
                channels: vec![ChannelInfo {
                    group: 0,
                    channel: 1,
                    name: "Vox".into(),
                    gain_db: -5.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
                ping_sequence: 0,
            },
        );
        let connected = cache.build_snapshot(&state);
        assert_eq!(connected.connection, AppConnectionState::Connected);
        assert_eq!(connected.current_scene.unwrap().name, "Bridge");
        assert_eq!(connected.scenes.len(), 1);
        assert_eq!(connected.channel_count, 1);

        let connected_identity = Lv1SystemIdentity {
            uuid: Some("connected-uuid".to_string()),
            host: Some("lv1.local".to_string()),
            address: "192.0.2.10".to_string(),
            port: 7788,
        };

        state.show = ShowProjectionState {
            lockout: false,
            show_file_path: None,
            show_file_name: "Untitled Session".to_string(),
            show_file_dirty: false,
            show_file_last_saved_at: None,
            discovered_lv1_systems: Vec::new(),
            connected_lv1_identity: Some(connected_identity.clone()),
            last_event_at: None,
        };

        let changed = cache.apply_lv1_event(
            0,
            &Lv1Event::Disconnected {
                reason: "link lost".to_string(),
            },
        );

        let snapshot = cache.build_snapshot(&state);

        assert!(changed);
        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert_eq!(snapshot.connected_lv1_identity, Some(connected_identity));
        assert!(snapshot.current_scene.is_none());
        assert!(snapshot.scenes.is_empty());
        assert!(snapshot.channels.is_empty());
        assert_eq!(snapshot.channel_count, 0);
    }

    #[test]
    fn cache_applies_scenes_projection_state_separately_from_show_state() {
        let mut cache = ProjectionCache::new();
        let state = AppStateSnapshot {
            show: ShowProjectionState {
                lockout: true,
                show_file_path: None,
                show_file_name: "Untitled Session".to_string(),
                show_file_dirty: false,
                show_file_last_saved_at: None,
                discovered_lv1_systems: Vec::new(),
                connected_lv1_identity: None,
                last_event_at: None,
            },
            scenes: crate::scenes::ScenesProjectionState {
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
            },
            ..Default::default()
        };

        let snapshot = cache.build_snapshot(&state);

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
        let mut state = AppStateSnapshot::default();
        let cue_list_id = uuid::Uuid::from_u128(1);
        state.cue_lists = CueListsProjectionState {
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
        };

        let snapshot = cache.build_snapshot(&state);

        assert_eq!(snapshot.cue_lists[0].name, "Main");
        assert_eq!(
            snapshot.active_cue_list_id.as_deref(),
            Some(cue_list_id.to_string().as_str())
        );
    }

    #[test]
    fn cache_applies_fade_state_events() {
        let mut cache = ProjectionCache::new();
        let state = AppStateSnapshot::default();

        assert!(cache.apply_fade_event(0, &FadeEvent::FadeStarted));
        assert_eq!(
            cache.build_snapshot(&state).fade_state,
            AppFadeState::Running
        );

        assert!(cache.apply_fade_event(
            0,
            &FadeEvent::ChannelOverride {
                group: 1,
                channel: 1,
                parameter: FadeParameter::FaderDb,
            }
        ));
        assert_eq!(
            cache.build_snapshot(&state).fade_state,
            AppFadeState::Blocked
        );

        assert!(cache.apply_fade_event(0, &FadeEvent::FadeCompleted));
        assert_eq!(cache.build_snapshot(&state).fade_state, AppFadeState::Idle);
    }

    #[test]
    fn cache_ignores_stale_generation_fade_events() {
        let mut cache = ProjectionCache::new();
        let state = AppStateSnapshot::default();
        cache.set_active_generation(2);

        assert!(!cache.apply_fade_event(1, &FadeEvent::FadeStarted));

        assert_eq!(cache.build_snapshot(&state).fade_state, AppFadeState::Idle);
    }

    #[test]
    fn cache_keeps_fade_state_when_channel_cancelled() {
        let mut cache = ProjectionCache::new();
        let state = AppStateSnapshot::default();

        cache.apply_fade_event(0, &FadeEvent::FadeStarted);
        cache.apply_fade_event(
            0,
            &FadeEvent::ChannelCancelled {
                group: 1,
                channel: 1,
                parameter: FadeParameter::FaderDb,
            },
        );

        assert_eq!(
            cache.build_snapshot(&state).fade_state,
            AppFadeState::Running
        );
    }

    #[test]
    fn cache_owns_bounded_log_entries() {
        let mut cache = ProjectionCache::new();
        let state = AppStateSnapshot::default();

        for index in 0..(MAX_PROJECTOR_LOGS + 2) {
            cache.append_log(UiLogEvent {
                severity: LogSeverity::Info,
                message: format!("log {index}"),
            });
        }

        let snapshot = cache.build_snapshot(&state);

        assert_eq!(snapshot.logs.len(), MAX_PROJECTOR_LOGS);
        assert_eq!(snapshot.logs[0].id, 3);
        assert_eq!(snapshot.logs.last().unwrap().message, "log 201");
    }
}

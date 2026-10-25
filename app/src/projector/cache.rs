use std::collections::VecDeque;

use crate::logging::UiLogEvent;
use crate::projector::{AppLogEntry, AppViewState};
use crate::runtime::AppStateSnapshot;

pub const MAX_PROJECTOR_LOGS: usize = 200;

/// @cc [owner:mixxorz,label:architecture;state] projector-cache-ownership
/// The cache MUST own only bounded native UI logs and snapshot/log counters; all projected runtime
/// display and app-lifetime state MUST be read from `AppStateSnapshot` when a view is built rather
/// than copied into this cache.
#[derive(Debug)]
pub struct ProjectionCache {
    state_version: u64,
    logs: VecDeque<AppLogEntry>,
    next_log_id: u64,
}

impl Default for ProjectionCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectionCache {
    pub fn new() -> Self {
        Self {
            state_version: 0,
            logs: VecDeque::new(),
            next_log_id: 1,
        }
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
     * Every snapshot MUST combine the cache's logs with all display and app-lifetime fields from the
     * supplied retained app-state snapshot. Generation resets and LV1 disconnects MUST NOT remove
     * retained app-lifetime state.
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

        AppViewState {
            connection: state.display.connection.clone(),
            discovered_lv1_systems: state.show.discovered_lv1_systems.clone(),
            connected_lv1_identity: state.show.connected_lv1_identity.clone(),
            current_scene: state.display.current_scene.clone(),
            scene_count: state.display.scenes.len(),
            scenes: state.display.scenes.clone(),
            channel_count: state.display.channels.len(),
            channels: state.display.channels.clone(),
            fade_state: state.display.fade_state.clone(),
            lockout: state.show.lockout,
            scene_configs: state.scenes.scene_configs.clone(),
            scene_settings_clipboard_available: state.scenes.scene_settings_clipboard_available,
            cue_lists: state.cue_lists.document.cue_lists.clone(),
            session_revision: state.session_revision,
            active_cue_list_id: state.cue_lists.document.active_cue_list_id,
            current_cue_entry_id: state.cue_lists.current_cue_entry_id,
            cued_cue_entry_id: state.cue_lists.document.cued_cue_entry_id,
            selected_scene_internal_id: state.scenes.selected_scene_internal_id,
            show_file_name: state.show.show_file_name.clone(),
            show_file_path: state
                .show
                .show_file_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            show_file_dirty: state.show.show_file_dirty,
            show_file_last_saved_at: state.show.show_file_last_saved_at.clone(),
            settings: state.settings.clone(),
            updates: state.updates.clone(),
            logs: self.logs.iter().cloned().collect(),
            last_event_at: state.show.last_event_at.clone(),
            state_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection_state::Lv1SystemIdentity;
    use crate::cue_lists::CueListsProjectionState;
    use crate::fade::{FadeEvent, FadeParameter};
    use crate::lv1::{ChannelInfo, Lv1Event, SceneState};
    use crate::projector::{AppConnectionState, AppFadeState, LogSeverity};
    use crate::runtime::events::{AppEvent, AppEventBus};
    use crate::scenes::ScenesProjectionState;
    use crate::settings::{AppSettings, SettingsEvent};
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
    fn cache_projects_retained_lv1_scene_and_topology() {
        let mut cache = ProjectionCache::new();
        let event_bus = AppEventBus::default();
        let state = event_bus.state();

        event_bus.publish_lv1(0, Lv1Event::Connected);
        event_bus.publish_lv1(
            0,
            Lv1Event::SceneChanged(crate::lv1::SceneObservation {
                sequence: 1,
                scene: SceneState {
                    index: 3,
                    name: "Bridge".to_string(),
                },
            }),
        );
        event_bus.publish_lv1(
            0,
            Lv1Event::ChannelTopologyChanged(vec![ChannelInfo {
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

        let snapshot = cache.build_snapshot(&state.borrow());

        assert_eq!(snapshot.connection, AppConnectionState::Connected);
        assert_eq!(snapshot.current_scene.unwrap().name, "Bridge");
        assert_eq!(snapshot.channel_count, 1);
        assert_eq!(snapshot.channels[0].name, "Vox");

        // A newly started projector reads the same latest display without replay or actor queries.
        let latest = ProjectionCache::new().build_snapshot(&state.borrow());
        assert_eq!(latest.connection, snapshot.connection);
        assert_eq!(latest.channels, snapshot.channels);
    }

    #[test]
    fn generation_reset_preserves_app_lifetime_state_logs_and_versions() {
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
                selected_scene_internal_id: Some(uuid::Uuid::from_u128(1)),
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
            current_cue_entry_id: None,
        };
        let event_bus = AppEventBus::default();
        event_bus.publish(AppEvent::Show(state.show));
        event_bus.publish_scenes(
            0,
            crate::scenes::ScenesEvent::StateChanged {
                state: state.scenes,
                persisted_scene_edit: false,
            },
        );
        event_bus.publish(AppEvent::CueLists(state.cue_lists));
        event_bus.publish(AppEvent::Settings(SettingsEvent::StateChanged {
            settings: state.settings,
        }));
        let state = event_bus.state();
        cache.append_log(UiLogEvent {
            severity: LogSeverity::Info,
            message: "kept".to_string(),
        });
        event_bus.publish_lv1(0, Lv1Event::Connected);
        event_bus.publish_lv1(
            0,
            Lv1Event::SceneChanged(crate::lv1::SceneObservation {
                sequence: 1,
                scene: SceneState {
                    index: 1,
                    name: "A".to_string(),
                },
            }),
        );
        event_bus.publish_lv1(
            0,
            Lv1Event::SceneListChanged(vec![crate::lv1::SceneListEntry {
                index: 1,
                name: "A".to_string(),
            }]),
        );
        event_bus.publish_lv1(
            0,
            Lv1Event::ChannelTopologyChanged(vec![ChannelInfo {
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
        event_bus.publish_fade(0, FadeEvent::FadeStarted);
        let version_a = cache.build_snapshot(&state.borrow()).state_version;

        event_bus.publish_runtime_generation_changed(1);
        let snapshot = cache.build_snapshot(&state.borrow());

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);

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
            snapshot.selected_scene_internal_id,
            Some(uuid::Uuid::from_u128(1))
        );
        assert_eq!(snapshot.logs[0].message, "kept");
        assert!(snapshot.state_version > version_a);
    }

    #[test]
    fn parameter_only_events_do_not_change_projection() {
        let mut cache = ProjectionCache::new();
        let event_bus = AppEventBus::default();
        let mut state = event_bus.state();
        event_bus.publish_lv1(0, Lv1Event::Connected);
        let before = cache.build_snapshot(&state.borrow());
        state.borrow_and_update();
        for event in [
            Lv1Event::Connected,
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
            event_bus.publish_lv1(0, event);
            assert!(!state.has_changed().unwrap());
        }
        let after = cache.build_snapshot(&state.borrow());
        assert_eq!(before.connection, after.connection);
        assert!(after.channels.is_empty());
    }

    #[test]
    fn lv1_disconnect_clears_live_state_but_preserves_show_owned_metadata() {
        let mut cache = ProjectionCache::new();
        let event_bus = AppEventBus::default();
        let state = event_bus.state();
        event_bus.publish_lv1(0, Lv1Event::Connected);
        event_bus.publish_lv1(
            0,
            Lv1Event::SceneChanged(crate::lv1::SceneObservation {
                sequence: 1,
                scene: SceneState {
                    index: 3,
                    name: "Bridge".into(),
                },
            }),
        );
        event_bus.publish_lv1(
            0,
            Lv1Event::SceneListChanged(vec![crate::lv1::SceneListEntry {
                index: 3,
                name: "Bridge".into(),
            }]),
        );
        event_bus.publish_lv1(
            0,
            Lv1Event::ChannelTopologyChanged(vec![ChannelInfo {
                group: 0,
                channel: 1,
                name: "Vox".into(),
                gain_db: -5.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }]),
        );
        let connected = cache.build_snapshot(&state.borrow());
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

        event_bus.publish(AppEvent::Show(ShowProjectionState {
            lockout: false,
            show_file_path: None,
            show_file_name: "Untitled Session".to_string(),
            show_file_dirty: false,
            show_file_last_saved_at: None,
            discovered_lv1_systems: Vec::new(),
            connected_lv1_identity: Some(connected_identity.clone()),
            last_event_at: None,
        }));

        event_bus.publish_lv1(
            0,
            Lv1Event::Disconnected {
                reason: "link lost".to_string(),
            },
        );

        let snapshot = cache.build_snapshot(&state.borrow());

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
                selected_scene_internal_id: Some(uuid::Uuid::from_u128(
                    0x11111111111141118111111111111111,
                )),
                scene_settings_clipboard_available: true,
                ready_generation: Some(0),
            },
            ..Default::default()
        };

        let snapshot = cache.build_snapshot(&state);

        assert!(snapshot.lockout);
        assert_eq!(snapshot.scene_configs.len(), 1);
        assert_eq!(
            snapshot.selected_scene_internal_id,
            Some(uuid::Uuid::from_u128(0x11111111111141118111111111111111))
        );
        assert!(snapshot.scene_settings_clipboard_available);
    }

    #[test]
    fn cache_applies_cue_list_projection_state() {
        let mut cache = ProjectionCache::new();
        let mut state = AppStateSnapshot::default();
        let cue_list_id = uuid::Uuid::from_u128(1);
        let current_entry_id = uuid::Uuid::from_u128(2);
        let cued_entry_id = uuid::Uuid::from_u128(3);
        state.cue_lists = CueListsProjectionState {
            document: crate::cue_lists::CueListDocument {
                cue_lists: vec![crate::cue_lists::CueList {
                    id: cue_list_id,
                    name: "Main".to_string(),
                    entries: vec![],
                }],
                active_cue_list_id: Some(cue_list_id),
                cued_cue_entry_id: Some(cued_entry_id),
            },
            current_cue_entry_id: Some(current_entry_id),
        };

        let snapshot = cache.build_snapshot(&state);

        assert_eq!(snapshot.cue_lists[0].name, "Main");
        assert_eq!(snapshot.current_cue_entry_id, Some(current_entry_id));
        assert_eq!(snapshot.active_cue_list_id, Some(cue_list_id));
        assert_eq!(snapshot.cued_cue_entry_id, Some(cued_entry_id));

        // Typed identities retain the existing string representation at the serialized boundary.
        let serialized = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(serialized["activeCueListId"], cue_list_id.to_string());
        assert_eq!(
            serialized["currentCueEntryId"],
            current_entry_id.to_string()
        );
        assert_eq!(serialized["cuedCueEntryId"], cued_entry_id.to_string());
    }

    #[test]
    fn cache_projects_retained_fade_state() {
        let mut cache = ProjectionCache::new();
        let event_bus = AppEventBus::default();
        let state = event_bus.state();

        event_bus.publish_fade(0, FadeEvent::FadeStarted);
        assert_eq!(
            cache.build_snapshot(&state.borrow()).fade_state,
            AppFadeState::Running
        );

        event_bus.publish_fade(
            0,
            FadeEvent::ChannelOverride {
                group: 1,
                channel: 1,
                parameter: FadeParameter::FaderDb,
            },
        );
        assert_eq!(
            cache.build_snapshot(&state.borrow()).fade_state,
            AppFadeState::Blocked
        );

        event_bus.publish_fade(0, FadeEvent::FadeCompleted);
        assert_eq!(
            cache.build_snapshot(&state.borrow()).fade_state,
            AppFadeState::Idle
        );
    }

    #[test]
    fn stale_generation_facts_do_not_change_retained_display() {
        let mut cache = ProjectionCache::new();
        let event_bus = AppEventBus::default();
        let mut state = event_bus.state();
        event_bus.publish_runtime_generation_changed(2);
        state.borrow_and_update();

        event_bus.publish_fade(1, FadeEvent::FadeStarted);
        event_bus.publish_lv1(1, Lv1Event::Connected);
        assert!(!state.has_changed().unwrap());

        assert_eq!(
            cache.build_snapshot(&state.borrow()).fade_state,
            AppFadeState::Idle
        );
    }

    #[test]
    fn aggregate_neutral_fade_facts_do_not_notify_or_change_retained_display() {
        let mut cache = ProjectionCache::new();
        let event_bus = AppEventBus::default();
        let mut state = event_bus.state();

        event_bus.publish_fade(0, FadeEvent::FadeStarted);
        state.borrow_and_update();
        for event in [
            FadeEvent::FadeStarted,
            FadeEvent::ChannelCancelled {
                group: 1,
                channel: 1,
                parameter: FadeParameter::FaderDb,
            },
            FadeEvent::ChannelCompleted {
                group: 1,
                channel: 1,
                parameter: FadeParameter::FaderDb,
            },
            FadeEvent::WriteFailed {
                reason: "write failed".into(),
            },
        ] {
            event_bus.publish_fade(0, event);
            assert!(!state.has_changed().unwrap());
        }
        assert_eq!(
            cache.build_snapshot(&state.borrow()).fade_state,
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

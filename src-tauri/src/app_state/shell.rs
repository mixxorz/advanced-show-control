use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use lv1_scene_fade_utility::lv1::model::{ConnectionStatus, Lv1StateSnapshot};
use tokio::sync::Mutex;

use super::view::{
    AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, FadeTarget,
    LogSeverity, LogSource, SceneFadeConfig, SceneSummary,
};

pub(super) const MAX_LOGS: usize = 200;

#[derive(Default)]
pub struct RuntimeHandles {
    pub lv1: Option<lv1_scene_fade_utility::lv1::state::Lv1ActorHandle>,
    pub fade: Option<lv1_scene_fade_utility::fade::engine::FadeEngineHandle>,
}

#[derive(Clone)]
pub struct ShellState {
    pub handles: Arc<Mutex<RuntimeHandles>>,
    pub(super) inner: Arc<Mutex<ShellInner>>,
}

#[derive(Default)]
pub(super) struct ShellInner {
    pub(super) generation: u64,
    pub(super) lv1_snapshot: Option<Lv1StateSnapshot>,
    pub(super) fade_state: AppFadeState,
    pub(super) lockout: bool,
    pub(super) scene_fade_configs: Vec<SceneFadeConfig>,
    pub(super) selected_scene_id: Option<String>,
    pub(super) listen_mode_active: bool,
    pub(super) show_file_path: Option<PathBuf>,
    pub(super) show_file_dirty: bool,
    pub(super) show_file_last_saved_at: Option<String>,
    pub(super) unknown_fader_warnings: HashSet<(i32, i32)>,
    pub(super) logs: VecDeque<AppLogEntry>,
    pub(super) next_log_id: u64,
    pub(super) last_event_at: Option<String>,
}

impl Default for ShellState {
    fn default() -> Self {
        cover_state_variants();
        Self {
            handles: Arc::new(Mutex::new(RuntimeHandles::default())),
            inner: Arc::new(Mutex::new(ShellInner::default())),
        }
    }
}

impl ShellState {
    pub async fn snapshot(&self) -> AppViewState {
        let inner = self.inner.lock().await;
        snapshot_from_inner(&inner)
    }
}

fn cover_state_variants() {
    let _ = (
        LogSource::Fade,
        LogSeverity::Error,
        AppFadeState::Running,
        AppFadeState::Blocked,
    );
}

pub(super) fn scene_id(index: i32, name: &str) -> String {
    format!("{index}::{name}")
}

pub(super) fn snapshot_from_inner(inner: &ShellInner) -> AppViewState {
    let connection = inner
        .lv1_snapshot
        .as_ref()
        .map(|snapshot| match snapshot.connection {
            ConnectionStatus::Connecting => AppConnectionState::Connecting,
            ConnectionStatus::Connected => AppConnectionState::Connected,
            ConnectionStatus::Disconnected => AppConnectionState::Disconnected,
        })
        .unwrap_or(AppConnectionState::Disconnected);

    let current_scene = inner.lv1_snapshot.as_ref().and_then(|snapshot| {
        snapshot.scene.as_ref().map(|scene| SceneSummary {
            index: scene.index,
            name: scene.name.clone(),
        })
    });

    let scenes = inner
        .lv1_snapshot
        .as_ref()
        .map(|snapshot| {
            snapshot
                .scene_list
                .iter()
                .map(|scene| SceneSummary {
                    index: scene.index,
                    name: scene.name.clone(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let channel_count = inner
        .lv1_snapshot
        .as_ref()
        .map(|snapshot| snapshot.channels.len())
        .unwrap_or(0);

    let channels = inner
        .lv1_snapshot
        .as_ref()
        .map(|snapshot| {
            snapshot
                .channels
                .iter()
                .map(|channel| ChannelSummary {
                    group: channel.group,
                    channel: channel.channel,
                    name: channel.name.clone(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    AppViewState {
        connection,
        current_scene,
        scene_count: scenes.len(),
        scenes,
        channel_count,
        channels,
        fade_state: inner.fade_state.clone(),
        lockout: inner.lockout,
        scene_fade_configs: inner.scene_fade_configs.clone(),
        selected_scene_id: inner.selected_scene_id.clone(),
        listen_mode_active: inner.listen_mode_active,
        show_file_name: inner
            .show_file_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_else(|| "Untitled Show".to_string()),
        show_file_path: inner
            .show_file_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        show_file_dirty: inner.show_file_dirty,
        show_file_last_saved_at: inner.show_file_last_saved_at.clone(),
        logs: inner.logs.iter().cloned().collect(),
        last_event_at: inner.last_event_at.clone(),
    }
}

pub(super) fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    millis.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::show_file::DEFAULT_DURATION_MS;
    use lv1_scene_fade_utility::lv1::messages::Lv1Event;
    use lv1_scene_fade_utility::lv1::model::{ChannelInfo, SceneListEntry, SceneState};

    fn connected_snapshot() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
        }
    }

    fn connected_state_with_scene_and_channel() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }],
            channels: vec![ChannelInfo {
                group: 0,
                channel: 2,
                name: "Lead".to_string(),
                gain_db: -8.0,
                muted: false,
            }],
        }
    }

    #[tokio::test]
    async fn default_snapshot_exposes_untitled_show_and_is_not_dirty() {
        let state = ShellState::default();
        let snapshot = state.snapshot().await;

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert_eq!(snapshot.current_scene, None);
        assert_eq!(snapshot.scene_count, 0);
        assert_eq!(snapshot.channel_count, 0);
        assert!(snapshot.channels.is_empty());
        assert_eq!(snapshot.fade_state, AppFadeState::Idle);
        assert!(!snapshot.lockout);
        assert_eq!(snapshot.show_file_name, "Untitled Show");
        assert_eq!(snapshot.show_file_path, None);
        assert!(!snapshot.show_file_dirty);
        assert_eq!(snapshot.show_file_last_saved_at, None);
    }

    #[tokio::test]
    async fn lockout_is_owned_by_rust_state() {
        let state = ShellState::default();
        let snapshot = state.set_lockout(true).await;

        assert!(snapshot.lockout);
        assert_eq!(snapshot.logs.len(), 1);
        assert_eq!(snapshot.logs[0].message, "Lockout enabled");
    }

    #[tokio::test]
    async fn lockout_marks_show_file_dirty() {
        let state = ShellState::default();

        let snapshot = state.set_lockout(true).await;

        assert!(snapshot.show_file_dirty);
    }

    #[test]
    fn scene_list_reconciliation_creates_default_configs() {
        let mut inner = ShellInner::default();
        inner.reconcile_scene_fade_configs(&[
            SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            },
            SceneListEntry {
                index: 2,
                name: "Verse".to_string(),
            },
        ]);

        assert_eq!(inner.scene_fade_configs.len(), 2);
        assert_eq!(inner.scene_fade_configs[0].scene_id, "1::Intro");
        assert!(!inner.scene_fade_configs[0].fade_enabled);
        assert_eq!(inner.scene_fade_configs[0].duration_ms, DEFAULT_DURATION_MS);
        assert!(inner.scene_fade_configs[0].fade_targets.is_empty());
        assert_eq!(inner.selected_scene_id.as_deref(), Some("1::Intro"));
    }

    #[test]
    fn scene_list_reconciliation_preserves_matching_config_data() {
        let mut inner = ShellInner::default();
        inner.scene_fade_configs = vec![SceneFadeConfig {
            scene_id: "2::Verse".to_string(),
            scene_index: 2,
            scene_name: "Verse".to_string(),
            fade_enabled: true,
            duration_ms: DEFAULT_DURATION_MS,
            fade_targets: vec![FadeTarget {
                group: 0,
                channel: 4,
                channel_name: "Lead".to_string(),
                target_db: -5.5,
                enabled: true,
                updated_at: "123".to_string(),
            }],
        }];
        inner.selected_scene_id = Some("2::Verse".to_string());

        inner.reconcile_scene_fade_configs(&[
            SceneListEntry {
                index: 2,
                name: "Verse".to_string(),
            },
            SceneListEntry {
                index: 3,
                name: "Chorus".to_string(),
            },
        ]);

        let verse = inner
            .scene_fade_configs
            .iter()
            .find(|scene| scene.scene_id == "2::Verse")
            .unwrap();
        assert!(verse.fade_enabled);
        assert_eq!(verse.fade_targets.len(), 1);
        assert_eq!(inner.scene_fade_configs.len(), 2);
        assert_eq!(inner.selected_scene_id.as_deref(), Some("2::Verse"));
    }

    #[test]
    fn scene_list_reconciliation_turns_off_listen_mode_when_selected_scene_disappears() {
        let mut inner = ShellInner::default();
        inner.scene_fade_configs = vec![SceneFadeConfig {
            scene_id: "1::Intro".to_string(),
            scene_index: 1,
            scene_name: "Intro".to_string(),
            fade_enabled: false,
            duration_ms: DEFAULT_DURATION_MS,
            fade_targets: Vec::new(),
        }];
        inner.selected_scene_id = Some("1::Intro".to_string());
        inner.listen_mode_active = true;

        inner.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        }]);

        assert!(!inner.listen_mode_active);
        assert_eq!(inner.selected_scene_id.as_deref(), Some("2::Verse"));
        assert!(inner.logs.iter().any(|entry| entry.message
            == "Listen Mode stopped because selected scene is no longer available"));
    }

    #[test]
    fn scene_reconciliation_marks_loaded_show_dirty_when_scene_removed() {
        let mut inner = ShellInner::default();
        inner.show_file_path = Some(std::path::PathBuf::from("/tmp/test.lv1show"));
        inner.scene_fade_configs = vec![SceneFadeConfig {
            scene_id: "1::Intro".to_string(),
            scene_index: 1,
            scene_name: "Intro".to_string(),
            fade_enabled: true,
            duration_ms: DEFAULT_DURATION_MS,
            fade_targets: vec![FadeTarget {
                group: 0,
                channel: 2,
                channel_name: "Lead".to_string(),
                target_db: -5.0,
                enabled: true,
                updated_at: "123".to_string(),
            }],
        }];

        inner.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        }]);

        assert!(inner.show_file_dirty);
        assert_eq!(inner.scene_fade_configs.len(), 1);
        assert_eq!(inner.scene_fade_configs[0].scene_id, "2::Verse");
    }

    #[tokio::test]
    async fn set_scene_fade_enabled_marks_show_file_dirty() {
        let state = ShellState::default();
        state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }],
                channels: vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Lead".to_string(),
                    gain_db: -8.0,
                    muted: false,
                }],
            })
            .await;

        let snapshot = state
            .set_scene_fade_enabled("1::Intro".to_string(), true)
            .await
            .unwrap();

        assert!(snapshot.scene_fade_configs[0].fade_enabled);
        assert!(snapshot.show_file_dirty);
    }

    #[tokio::test]
    async fn set_scene_duration_ms_updates_duration_and_marks_dirty() {
        let state = ShellState::default();
        state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }],
                channels: vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Lead".to_string(),
                    gain_db: -8.0,
                    muted: false,
                }],
            })
            .await;

        let snapshot = state
            .set_scene_duration_ms("1::Intro".to_string(), 8_000)
            .await
            .unwrap();

        assert_eq!(snapshot.scene_fade_configs[0].duration_ms, 8_000);
        assert!(snapshot.show_file_dirty);
    }

    #[tokio::test]
    async fn export_show_file_contains_current_configs() {
        let state = ShellState::default();
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        state
            .set_scene_fade_enabled("1::Intro".to_string(), true)
            .await
            .unwrap();

        let file = state.export_show_file("saved".to_string()).await;

        assert_eq!(file.schema_version, 1);
        assert!(!file.safety.lockout);
        assert_eq!(file.saved_at, "saved");
        assert_eq!(file.scene_fade_configs[0].scene_index, 1);
        assert_eq!(file.scene_fade_configs[0].duration_ms, 4000);
    }

    #[tokio::test]
    async fn export_show_file_for_save_rejects_listen_mode() {
        let state = ShellState::default();
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        state.set_listen_mode(true).await.unwrap();

        assert_eq!(
            state
                .export_show_file_for_save("saved".to_string())
                .await
                .unwrap_err(),
            "Stop Listen Mode before saving a show file"
        );
    }

    #[tokio::test]
    async fn new_show_file_clears_file_state_and_rebuilds_current_lv1_scenes() {
        let state = ShellState::default();
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;

        {
            let mut inner = state.inner.lock().await;
            inner.scene_fade_configs[0].fade_enabled = true;
            inner.show_file_path = Some(std::path::PathBuf::from("/tmp/existing.lv1show"));
            inner.show_file_last_saved_at = Some("123".to_string());
            inner.show_file_dirty = true;
            inner.lockout = true;
        }

        let snapshot = state.new_show_file().await.unwrap();

        assert_eq!(snapshot.show_file_path, None);
        assert_eq!(snapshot.show_file_last_saved_at, None);
        assert!(!snapshot.show_file_dirty);
        assert!(!snapshot.lockout);
        assert_eq!(snapshot.scene_fade_configs.len(), 1);
        assert!(!snapshot.scene_fade_configs[0].fade_enabled);
        assert_eq!(
            snapshot.logs.last().unwrap().message,
            "New show file created"
        );
    }

    #[tokio::test]
    async fn current_show_file_path_returns_pathbuf() {
        let state = ShellState::default();
        let path = std::path::PathBuf::from("/tmp/test.lv1show");

        state
            .mark_show_file_saved(path.clone(), "999".to_string())
            .await;

        assert_eq!(state.current_show_file_path().await, Some(path));
    }

    #[tokio::test]
    async fn new_show_file_clears_stale_selection_when_disconnected() {
        let state = ShellState::default();

        {
            let mut inner = state.inner.lock().await;
            inner.selected_scene_id = Some("stale::scene".to_string());
            inner.scene_fade_configs = vec![SceneFadeConfig {
                scene_id: "stale::scene".to_string(),
                scene_index: 99,
                scene_name: "Stale".to_string(),
                fade_enabled: true,
                duration_ms: DEFAULT_DURATION_MS,
                fade_targets: Vec::new(),
            }];
        }

        let snapshot = state.new_show_file().await.unwrap();

        assert_eq!(snapshot.selected_scene_id, None);
        assert!(snapshot.scene_fade_configs.is_empty());
    }

    #[tokio::test]
    async fn new_show_file_rejects_listen_mode() {
        let state = ShellState::default();
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        state.set_listen_mode(true).await.unwrap();

        assert_eq!(
            state.new_show_file().await.unwrap_err(),
            "Stop Listen Mode before creating a new show file"
        );
    }

    #[tokio::test]
    async fn mark_show_file_saved_updates_path_and_clears_dirty() {
        let state = ShellState::default();

        let snapshot = state
            .mark_show_file_saved(
                std::path::PathBuf::from("/tmp/test.lv1show"),
                "999".to_string(),
            )
            .await;

        assert_eq!(
            snapshot.show_file_path.as_deref(),
            Some("/tmp/test.lv1show")
        );
        assert_eq!(snapshot.show_file_last_saved_at.as_deref(), Some("999"));
        assert!(!snapshot.show_file_dirty);
        assert_eq!(snapshot.logs.last().unwrap().message, "Show file saved");
    }

    #[tokio::test]
    async fn load_show_file_applies_kept_configs_and_logs_pruned_entries() {
        let state = ShellState::default();
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        let mut file = crate::show_file::ShowFile {
            schema_version: 1,
            app_version: "0.1.0".to_string(),
            saved_at: "123".to_string(),
            safety: crate::show_file::ShowFileSafety { lockout: true },
            scene_fade_configs: vec![
                crate::show_file::ShowFileSceneFadeConfig {
                    scene_index: 1,
                    scene_name: "Intro".to_string(),
                    fade_enabled: true,
                    duration_ms: 5000,
                    fade_targets: vec![crate::show_file::ShowFileFadeTarget {
                        group: 0,
                        channel: 2,
                        channel_name: "Lead".to_string(),
                        target_db: -9.0,
                        enabled: true,
                        updated_at: "999".to_string(),
                    }],
                },
                crate::show_file::ShowFileSceneFadeConfig {
                    scene_index: 2,
                    scene_name: "Missing".to_string(),
                    fade_enabled: true,
                    duration_ms: 5000,
                    fade_targets: Vec::new(),
                },
            ],
        };

        let snapshot = state
            .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), &mut file)
            .await
            .unwrap();

        assert!(snapshot.lockout);
        assert_eq!(snapshot.scene_fade_configs.len(), 1);
        assert_eq!(snapshot.scene_fade_configs[0].duration_ms, 5000);
        assert_eq!(
            snapshot.scene_fade_configs[0].fade_targets[0].channel_name,
            "Lead"
        );
        assert!(snapshot.show_file_dirty);
        assert!(snapshot.logs.iter().any(|entry| {
            entry.message == "Deleted saved scene config during load: 2: Missing"
        }));
    }

    #[tokio::test]
    async fn load_show_file_clears_unknown_fader_warnings() {
        let state = ShellState::default();
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        {
            let mut inner = state.inner.lock().await;
            inner.unknown_fader_warnings.insert((0, 99));
        }

        let mut file = crate::show_file::ShowFile {
            schema_version: 1,
            app_version: "0.1.0".to_string(),
            saved_at: "123".to_string(),
            safety: crate::show_file::ShowFileSafety { lockout: false },
            scene_fade_configs: vec![crate::show_file::ShowFileSceneFadeConfig {
                scene_index: 1,
                scene_name: "Intro".to_string(),
                fade_enabled: false,
                duration_ms: 4000,
                fade_targets: Vec::new(),
            }],
        };

        state
            .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), &mut file)
            .await
            .unwrap();

        let inner = state.inner.lock().await;
        assert!(inner.unknown_fader_warnings.is_empty());
    }

    #[test]
    fn scene_list_reconciliation_keeps_listen_mode_when_no_scene_was_selected() {
        let mut inner = ShellInner::default();
        inner.listen_mode_active = true;

        inner.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        }]);

        assert!(inner.listen_mode_active);
        assert_eq!(inner.selected_scene_id.as_deref(), Some("2::Verse"));
        assert!(!inner.logs.iter().any(|entry| entry.message
            == "Listen Mode stopped because selected scene is no longer available"));
    }

    #[tokio::test]
    async fn begin_connecting_sets_connecting_snapshot_and_logs_it() {
        let state = ShellState::default();

        let (generation, snapshot) = state.begin_connecting().await;

        assert_eq!(generation, 1);
        assert_eq!(snapshot.connection, AppConnectionState::Connecting);
        assert_eq!(snapshot.logs.len(), 1);
        assert_eq!(snapshot.logs[0].message, "Connecting to LV1");
    }

    #[test]
    fn snapshot_maps_lv1_scene_and_counts() {
        let mut inner = ShellInner::default();
        inner.lv1_snapshot = Some(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(SceneState {
                index: 3,
                name: "Verse".to_string(),
            }),
            scene_list: vec![SceneListEntry {
                index: 3,
                name: "Verse".to_string(),
            }],
            channels: vec![ChannelInfo {
                group: 0,
                channel: 0,
                name: "Lead".to_string(),
                gain_db: -6.0,
                muted: false,
            }],
        });

        let snapshot = snapshot_from_inner(&inner);

        assert_eq!(snapshot.connection, AppConnectionState::Connected);
        assert_eq!(snapshot.current_scene.unwrap().name, "Verse");
        assert_eq!(snapshot.scene_count, 1);
        assert_eq!(snapshot.channel_count, 1);
        assert_eq!(snapshot.channels.len(), 1);
        assert_eq!(snapshot.channels[0].group, 0);
        assert_eq!(snapshot.channels[0].channel, 0);
        assert_eq!(snapshot.channels[0].name, "Lead");
    }

    #[test]
    fn enum_variants_are_kept_for_state_space_coverage() {
        let _ = (
            LogSource::Fade,
            LogSeverity::Error,
            AppFadeState::Running,
            AppFadeState::Blocked,
        );
    }

    #[tokio::test]
    async fn lv1_scene_event_updates_rust_owned_snapshot() {
        let state = ShellState::default();
        let (generation, _snapshot) = state.begin_connecting().await;
        let snapshot = state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::SceneChanged(SceneState {
                    index: 7,
                    name: "Chorus".to_string(),
                }),
            )
            .await;

        let snapshot = snapshot.expect("event should apply to current generation");

        assert_eq!(snapshot.connection, AppConnectionState::Connecting);
        assert_eq!(snapshot.current_scene.unwrap().name, "Chorus");
        assert_eq!(snapshot.logs.len(), 2);
    }

    #[tokio::test]
    async fn begin_connection_preserves_incoming_connection_state() {
        let state = ShellState::default();
        let (_, _connecting) = state.begin_connecting().await;

        let snapshot = state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connecting,
                scene: None,
                scene_list: Vec::new(),
                channels: Vec::new(),
            })
            .await;

        assert_eq!(snapshot.connection, AppConnectionState::Connecting);
        assert_eq!(snapshot.logs.last().unwrap().message, "Connecting to LV1");

        let snapshot = state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: Vec::new(),
                channels: Vec::new(),
            })
            .await;

        assert_eq!(snapshot.connection, AppConnectionState::Connected);
        assert_eq!(snapshot.logs.last().unwrap().message, "LV1 connected");
    }

    #[tokio::test]
    async fn stale_lv1_events_are_ignored_after_generation_change() {
        let state = ShellState::default();

        let (first_generation, first_snapshot) = state.begin_connecting().await;
        assert_eq!(first_snapshot.connection, AppConnectionState::Connecting);

        let (second_generation, second_connecting) = state.begin_connecting().await;
        assert_eq!(second_generation, first_generation + 1);
        assert_eq!(second_connecting.connection, AppConnectionState::Connecting);

        let second_snapshot = state
            .begin_connection(Lv1StateSnapshot {
                scene: None,
                scene_list: vec![],
                channels: vec![],
                connection: ConnectionStatus::Connected,
            })
            .await;
        assert_eq!(second_snapshot.connection, AppConnectionState::Connected);

        let stale = state
            .apply_lv1_event_for_generation(
                first_generation,
                &Lv1Event::SceneChanged(SceneState {
                    index: 5,
                    name: "Intro".to_string(),
                }),
            )
            .await;
        assert!(stale.is_none());

        let current = state
            .apply_lv1_event_for_generation(
                second_generation,
                &Lv1Event::SceneChanged(SceneState {
                    index: 6,
                    name: "Bridge".to_string(),
                }),
            )
            .await;
        assert!(current.is_some());

        let latest = current.expect("event should apply to current generation");
        assert_eq!(latest.current_scene.unwrap().name, "Bridge");
    }

    #[tokio::test]
    async fn disconnect_increments_generation_and_ignores_old_events() {
        let state = ShellState::default();
        let (generation, snapshot) = state.begin_connecting().await;
        assert_eq!(snapshot.connection, AppConnectionState::Connecting);

        let snapshot = state.begin_connection(connected_snapshot()).await;
        assert_eq!(snapshot.connection, AppConnectionState::Connected);

        let disconnected = state.disconnect().await;
        assert_eq!(disconnected.connection, AppConnectionState::Disconnected);

        let stale = state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::SceneChanged(SceneState {
                    index: 9,
                    name: "Outro".to_string(),
                }),
            )
            .await;
        assert!(stale.is_none());
    }

    #[tokio::test]
    async fn disconnect_turns_off_listen_mode() {
        let state = ShellState::default();
        let _ = state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }],
                channels: Vec::new(),
            })
            .await;

        {
            let mut inner = state.inner.lock().await;
            inner.listen_mode_active = true;
        }

        let snapshot = state.disconnect().await;

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert!(!snapshot.listen_mode_active);
    }

    #[tokio::test]
    async fn listen_mode_requires_selected_scene_and_known_channels() {
        let state = ShellState::default();

        let err = state.set_listen_mode(true).await.unwrap_err();
        assert_eq!(err, "Select a scene before starting Listen Mode");

        let snapshot = state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }],
                channels: Vec::new(),
            })
            .await;
        assert_eq!(snapshot.selected_scene_id.as_deref(), Some("1::Intro"));

        let err = state.set_listen_mode(true).await.unwrap_err();
        assert_eq!(err, "LV1 channel list is empty");
    }

    #[tokio::test]
    async fn fader_events_write_targets_only_while_listen_mode_is_active() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -4.5,
                },
            )
            .await;
        assert!(
            state.snapshot().await.scene_fade_configs[0]
                .fade_targets
                .is_empty()
        );

        state.set_listen_mode(true).await.unwrap();
        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -4.5,
                },
            )
            .await;

        let view = state.snapshot().await;
        let targets = &view.scene_fade_configs[0].fade_targets;
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].group, 0);
        assert_eq!(targets[0].channel, 2);
        assert_eq!(targets[0].channel_name, "Lead");
        assert_eq!(targets[0].target_db, -4.5);
        assert!(targets[0].enabled);
        assert!(view.show_file_dirty);
    }

    #[tokio::test]
    async fn repeated_fader_event_updates_existing_target() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        state.set_listen_mode(true).await.unwrap();

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -4.5,
                },
            )
            .await;
        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -3.0,
                },
            )
            .await;

        let targets = &state.snapshot().await.scene_fade_configs[0].fade_targets;
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_db, -3.0);
        assert_eq!(targets[0].channel_name, "Lead");
    }

    #[tokio::test]
    async fn repeated_fader_event_preserves_disabled_target_state() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        state.set_listen_mode(true).await.unwrap();

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -4.5,
                },
            )
            .await;
        state
            .set_fade_target_enabled("1::Intro".to_string(), 0, 2, false)
            .await
            .unwrap();

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -3.25,
                },
            )
            .await;

        let targets = &state.snapshot().await.scene_fade_configs[0].fade_targets;
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_db, -3.25);
        assert!(!targets[0].enabled);
        assert_eq!(targets[0].channel_name, "Lead");
    }

    #[tokio::test]
    async fn removed_target_can_be_recaptured_while_listen_mode_is_active() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        state.set_listen_mode(true).await.unwrap();

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -4.5,
                },
            )
            .await;
        state.remove_fade_target("1::Intro", 0, 2).await.unwrap();
        let snapshot = state.snapshot().await;
        assert!(snapshot.scene_fade_configs[0].fade_targets.is_empty());
        assert!(snapshot.show_file_dirty);

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 2,
                    gain_db: -2.0,
                },
            )
            .await;
        let targets = &state.snapshot().await.scene_fade_configs[0].fade_targets;
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_db, -2.0);
    }

    #[tokio::test]
    async fn unknown_channel_fader_warning_logs_only_once_per_channel() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        state.set_listen_mode(true).await.unwrap();

        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 99,
                    gain_db: -1.0,
                },
            )
            .await;
        state
            .apply_lv1_event_for_generation(
                generation,
                &Lv1Event::FaderChanged {
                    group: 0,
                    channel: 99,
                    gain_db: -2.0,
                },
            )
            .await;

        let warnings: Vec<_> = state
            .snapshot()
            .await
            .logs
            .into_iter()
            .filter(|entry| entry.message == "Ignored fader target for unknown channel 0/99")
            .collect();
        assert_eq!(warnings.len(), 1);
    }

    #[tokio::test]
    async fn disconnect_turns_off_listen_mode_and_preserves_configs() {
        let state = ShellState::default();
        state
            .begin_connection(connected_state_with_scene_and_channel())
            .await;
        state.set_listen_mode(true).await.unwrap();

        let view = state.disconnect().await;

        assert!(!view.listen_mode_active);
        assert_eq!(view.scene_fade_configs.len(), 1);
    }

    #[tokio::test]
    async fn lv1_disconnected_event_turns_off_listen_mode() {
        let state = ShellState::default();
        let (generation, _) = state.begin_connecting().await;
        let _ = state
            .begin_connection(Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }],
                channels: Vec::new(),
            })
            .await;

        {
            let mut inner = state.inner.lock().await;
            inner.listen_mode_active = true;
        }

        let snapshot = state
            .apply_lv1_event_for_generation(generation, &Lv1Event::Disconnected)
            .await
            .expect("event should apply to current generation");

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert!(!snapshot.listen_mode_active);
    }
}

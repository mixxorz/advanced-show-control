use super::shell::ShellState;
use super::test_support::{begin_test_connection, connected_state_with_scene_and_channel};
use super::view::ShowSnapshot;
use crate::show_file::{ShowFile, ShowFileChannelConfig, ShowFileChannelRef, ShowFileSceneConfig};
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::{LookupSpan, Registry};

#[derive(Clone, Debug, PartialEq, Eq)]
struct CapturedWarning {
    event: String,
    message: String,
    scene: String,
}

struct CapturedWarnings(Arc<Mutex<Vec<CapturedWarning>>>);

impl<S> Layer<S> for CapturedWarnings
where
    S: tracing::Subscriber,
    S: for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        if *event.metadata().level() != tracing::Level::WARN {
            return;
        }

        let mut visitor = WarningVisitor::default();
        event.record(&mut visitor);
        if let (Some(event), Some(message), Some(scene)) =
            (visitor.event, visitor.message, visitor.scene)
        {
            self.0.lock().unwrap().push(CapturedWarning {
                event,
                message,
                scene,
            });
        }
    }
}

#[derive(Default)]
struct WarningVisitor {
    event: Option<String>,
    message: Option<String>,
    scene: Option<String>,
}

impl WarningVisitor {
    fn record_field(&mut self, name: &str, value: String) {
        match name {
            "event" => self.event = Some(value),
            "message" => self.message = Some(value),
            "scene" => self.scene = Some(value),
            _ => {}
        }
    }
}

impl Visit for WarningVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_field(field.name(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_field(field.name(), format!("{value:?}"));
    }
}

fn populated_show_snapshot() -> ShowSnapshot {
    ShowSnapshot {
        lockout: true,
        cued_scene_id: None,
        scene_configs: vec![super::view::SceneConfig {
            scene_id: "1::Intro".to_string(),
            scene_index: 1,
            scene_name: "Intro".to_string(),
            duration_ms: 5000,
            channel_configs: vec![super::view::ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-8.0),
                pan: Some(-12.0),
                balance: Some(3.0),
                width: Some(1.2),
                pan_mode: Some(crate::lv1::types::PanMode::Stereo),
            }],
            scoped_channels: vec![super::view::ChannelRef {
                group: 0,
                channel: 2,
            }],
            scope_toggles: crate::show::types::SceneScopeToggles {
                faders: false,
                pan: true,
            },
        }],
    }
}

#[tokio::test]
async fn export_show_file_contains_current_configs() {
    let state = ShellState::default();
    begin_test_connection(&state, connected_state_with_scene_and_channel()).await;
    state.show.replace_snapshot(populated_show_snapshot()).await;

    let file = state.export_show_file("saved".to_string()).await;

    assert_eq!(file.schema_version, 1);
    assert!(file.safety.lockout);
    assert_eq!(file.saved_at, "saved");
    assert_eq!(file.scene_configs[0].scene_index, 1);
    assert_eq!(file.scene_configs[0].duration_ms, 5000);
    assert_eq!(
        file.scene_configs[0].channel_configs,
        vec![ShowFileChannelConfig {
            group: 0,
            channel: 2,
            fader_db: Some(-8.0),
            pan: Some(-12.0),
            balance: Some(3.0),
            width: Some(1.2),
            pan_mode: Some(crate::lv1::types::PanMode::Stereo),
        }]
    );
    assert_eq!(
        file.scene_configs[0].scoped_channels,
        vec![ShowFileChannelRef {
            group: 0,
            channel: 2
        }]
    );
    assert!(!file.scene_configs[0].scope_toggles.faders);
    assert!(file.scene_configs[0].scope_toggles.pan);
}

#[tokio::test]
async fn export_load_export_round_trips_show_file_mapping() {
    let source = ShellState::default();
    begin_test_connection(&source, connected_state_with_scene_and_channel()).await;
    source
        .show
        .replace_snapshot(populated_show_snapshot())
        .await;

    let exported = source.export_show_file("saved".to_string()).await;

    let target = ShellState::default();
    begin_test_connection(&target, connected_state_with_scene_and_channel()).await;
    let mut imported = exported.clone();
    target
        .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), &mut imported)
        .await
        .unwrap();

    let round_tripped = target.export_show_file("saved-again".to_string()).await;

    assert_eq!(round_tripped.schema_version, exported.schema_version);
    assert_eq!(round_tripped.safety, exported.safety);
    assert_eq!(round_tripped.scene_configs, exported.scene_configs);
}

fn lv1_scene_only_snapshot() -> crate::lv1::types::Lv1StateSnapshot {
    crate::lv1::types::Lv1StateSnapshot {
        connection: crate::lv1::types::ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![crate::lv1::types::SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }],
        channels: Vec::new(),
    }
}

#[tokio::test]
async fn new_show_file_clears_file_state_and_rebuilds_current_lv1_scenes() {
    let state = ShellState::default();
    begin_test_connection(&state, lv1_scene_only_snapshot()).await;
    state
        .show
        .replace_snapshot(ShowSnapshot {
            lockout: true,
            cued_scene_id: None,
            scene_configs: vec![super::view::SceneConfig {
                scene_id: "1::Intro".to_string(),
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms: 0,
                channel_configs: vec![super::view::ChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-8.0),
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
                scoped_channels: vec![super::view::ChannelRef {
                    group: 0,
                    channel: 2,
                }],
                scope_toggles: crate::show::types::SceneScopeToggles::default(),
            }],
        })
        .await;

    let snapshot = state.new_show_file().await.unwrap();

    assert_eq!(snapshot.show_file_path, None);
    assert_eq!(snapshot.show_file_last_saved_at, None);
    assert!(!snapshot.show_file_dirty);
    assert!(!snapshot.lockout);
    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].duration_ms, 0);
    assert!(snapshot.scene_configs[0].channel_configs.is_empty());
    assert!(snapshot.scene_configs[0].scoped_channels.is_empty());
    assert!(
        snapshot
            .logs
            .iter()
            .all(|entry| entry.message != "New show file created")
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
    state
        .show
        .replace_snapshot(ShowSnapshot {
            lockout: false,
            cued_scene_id: None,
            scene_configs: vec![super::view::SceneConfig {
                scene_id: "stale::scene".to_string(),
                scene_index: 99,
                scene_name: "Stale".to_string(),
                duration_ms: 0,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: crate::show::types::SceneScopeToggles::default(),
            }],
        })
        .await;
    {
        let mut inner = state.inner.lock().await;
        inner.selected_scene_id = Some("stale::scene".to_string());
    }

    let snapshot = state.new_show_file().await.unwrap();

    assert_eq!(snapshot.selected_scene_id, None);
    assert!(snapshot.scene_configs.is_empty());
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
    assert!(
        snapshot
            .logs
            .iter()
            .all(|entry| entry.message != "Show file saved")
    );
}

#[tokio::test]
async fn load_show_file_applies_kept_configs_and_logs_pruned_entries() {
    let state = ShellState::default();
    begin_test_connection(&state, connected_state_with_scene_and_channel()).await;
    let mut file = ShowFile {
        schema_version: 1,
        app_version: "0.1.0".to_string(),
        saved_at: "123".to_string(),
        safety: crate::show_file::ShowFileSafety { lockout: true },
        cued_scene_id: None,
        scene_configs: vec![
            ShowFileSceneConfig {
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms: 5000,
                channel_configs: vec![ShowFileChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-9.0),
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
                scoped_channels: vec![ShowFileChannelRef {
                    group: 0,
                    channel: 2,
                }],
                scope_toggles: crate::show_file::ShowFileSceneScopeToggles::default(),
            },
            ShowFileSceneConfig {
                scene_index: 2,
                scene_name: "Missing".to_string(),
                duration_ms: 5000,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: crate::show_file::ShowFileSceneScopeToggles::default(),
            },
        ],
    };

    let captured = Arc::new(Mutex::new(Vec::new()));
    let subscriber = Registry::default().with(CapturedWarnings(captured.clone()));
    let _guard = tracing::subscriber::set_default(subscriber);

    let snapshot = state
        .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), &mut file)
        .await
        .unwrap();

    assert!(snapshot.lockout);
    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].duration_ms, 5000);
    assert_eq!(snapshot.scene_configs[0].channel_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].scoped_channels.len(), 1);
    assert!(snapshot.show_file_dirty);
    assert!(
        snapshot
            .logs
            .iter()
            .all(|entry| entry.message != "Deleted saved scene config during load: 2: Missing")
    );
    assert_eq!(
        captured.lock().unwrap().as_slice(),
        &[CapturedWarning {
            event: "show_file_scene_pruned".to_string(),
            message:
                "Skipped loading \"2: Missing\" because it was not found in the current scene list."
                    .to_string(),
            scene: "2: Missing".to_string(),
        }]
    );
}

#[tokio::test]
async fn load_show_file_preserves_disabled_fader_scope_toggle() {
    let state = ShellState::default();
    begin_test_connection(&state, connected_state_with_scene_and_channel()).await;

    let mut file = ShowFile {
        schema_version: 1,
        app_version: "0.1.0".to_string(),
        saved_at: "123".to_string(),
        safety: crate::show_file::ShowFileSafety { lockout: false },
        cued_scene_id: None,
        scene_configs: vec![ShowFileSceneConfig {
            scene_index: 1,
            scene_name: "Intro".to_string(),
            duration_ms: 5000,
            channel_configs: vec![ShowFileChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-9.0),
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            scoped_channels: vec![ShowFileChannelRef {
                group: 0,
                channel: 2,
            }],
            scope_toggles: crate::show_file::ShowFileSceneScopeToggles {
                faders: false,
                pan: false,
            },
        }],
    };

    let snapshot = state
        .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), &mut file)
        .await
        .unwrap();

    assert!(!snapshot.scene_configs[0].scope_toggles.faders);
    assert!(!snapshot.scene_configs[0].scope_toggles.pan);
}

#[tokio::test]
async fn export_and_import_show_file_round_trips_pan_family_fields() {
    let state = ShellState::default();
    begin_test_connection(&state, connected_state_with_scene_and_channel()).await;
    state
        .show
        .replace_snapshot(ShowSnapshot {
            lockout: true,
            cued_scene_id: None,
            scene_configs: vec![super::view::SceneConfig {
                scene_id: "1::Intro".to_string(),
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms: 5000,
                channel_configs: vec![super::view::ChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-8.0),
                    pan: Some(-12.0),
                    balance: Some(3.0),
                    width: Some(1.2),
                    pan_mode: Some(crate::lv1::types::PanMode::Stereo),
                }],
                scoped_channels: vec![super::view::ChannelRef {
                    group: 0,
                    channel: 2,
                }],
                scope_toggles: crate::show::types::SceneScopeToggles {
                    faders: true,
                    pan: true,
                },
            }],
        })
        .await;

    let exported = state.export_show_file("saved".to_string()).await;
    assert!(exported.scene_configs[0].scope_toggles.pan);
    assert_eq!(
        exported.scene_configs[0].channel_configs[0].pan,
        Some(-12.0)
    );
    assert_eq!(
        exported.scene_configs[0].channel_configs[0].balance,
        Some(3.0)
    );
    assert_eq!(
        exported.scene_configs[0].channel_configs[0].width,
        Some(1.2)
    );
    assert_eq!(
        exported.scene_configs[0].channel_configs[0].pan_mode,
        Some(crate::lv1::types::PanMode::Stereo)
    );

    let mut imported = exported.clone();
    let snapshot = state
        .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), &mut imported)
        .await
        .unwrap();

    assert!(snapshot.scene_configs[0].scope_toggles.pan);
    assert_eq!(
        snapshot.scene_configs[0].channel_configs[0].pan,
        Some(-12.0)
    );
    assert_eq!(
        snapshot.scene_configs[0].channel_configs[0].balance,
        Some(3.0)
    );
    assert_eq!(
        snapshot.scene_configs[0].channel_configs[0].width,
        Some(1.2)
    );
    assert_eq!(
        snapshot.scene_configs[0].channel_configs[0].pan_mode,
        Some(crate::lv1::types::PanMode::Stereo)
    );
}

#[tokio::test]
async fn load_show_file_defaults_missing_pan_family_fields() {
    let state = ShellState::default();
    begin_test_connection(&state, connected_state_with_scene_and_channel()).await;

    let mut file = ShowFile {
        schema_version: 1,
        app_version: "0.1.0".to_string(),
        saved_at: "123".to_string(),
        safety: crate::show_file::ShowFileSafety { lockout: false },
        cued_scene_id: None,
        scene_configs: vec![ShowFileSceneConfig {
            scene_index: 1,
            scene_name: "Intro".to_string(),
            duration_ms: 5000,
            channel_configs: vec![ShowFileChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-9.0),
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            scoped_channels: vec![ShowFileChannelRef {
                group: 0,
                channel: 2,
            }],
            scope_toggles: crate::show_file::ShowFileSceneScopeToggles::default(),
        }],
    };

    let snapshot = state
        .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), &mut file)
        .await
        .unwrap();

    assert!(!snapshot.scene_configs[0].scope_toggles.pan);
    assert_eq!(snapshot.scene_configs[0].channel_configs[0].pan, None);
    assert_eq!(snapshot.scene_configs[0].channel_configs[0].balance, None);
    assert_eq!(snapshot.scene_configs[0].channel_configs[0].width, None);
    assert_eq!(snapshot.scene_configs[0].channel_configs[0].pan_mode, None);
}

#[tokio::test]
async fn load_show_file_allows_empty_lv1_channels_when_scenes_exist() {
    let state = ShellState::default();
    begin_test_connection(&state, lv1_scene_only_snapshot()).await;

    let mut file = ShowFile {
        schema_version: 1,
        app_version: "0.1.0".to_string(),
        saved_at: "123".to_string(),
        safety: crate::show_file::ShowFileSafety { lockout: false },
        cued_scene_id: None,
        scene_configs: vec![ShowFileSceneConfig {
            scene_index: 1,
            scene_name: "Intro".to_string(),
            duration_ms: 5000,
            channel_configs: vec![ShowFileChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-9.0),
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            scoped_channels: vec![ShowFileChannelRef {
                group: 0,
                channel: 2,
            }],
            scope_toggles: crate::show_file::ShowFileSceneScopeToggles::default(),
        }],
    };

    let snapshot = state
        .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), &mut file)
        .await
        .unwrap();

    assert!(!snapshot.show_file_dirty);
    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].channel_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].scoped_channels.len(), 1);
}

#[tokio::test]
async fn load_show_file_clears_pruned_cued_scene_id() {
    let state = ShellState::default();
    begin_test_connection(&state, lv1_scene_only_snapshot()).await;

    let mut file = ShowFile {
        schema_version: 1,
        app_version: "0.1.0".to_string(),
        saved_at: "123".to_string(),
        safety: crate::show_file::ShowFileSafety { lockout: false },
        cued_scene_id: Some("2::Unused".to_string()),
        scene_configs: vec![
            ShowFileSceneConfig {
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms: 5000,
                channel_configs: vec![ShowFileChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-9.0),
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
                scoped_channels: vec![ShowFileChannelRef {
                    group: 0,
                    channel: 2,
                }],
                scope_toggles: crate::show_file::ShowFileSceneScopeToggles::default(),
            },
            ShowFileSceneConfig {
                scene_index: 2,
                scene_name: "Unused".to_string(),
                duration_ms: 6000,
                channel_configs: vec![],
                scoped_channels: vec![],
                scope_toggles: crate::show_file::ShowFileSceneScopeToggles::default(),
            },
        ],
    };

    let snapshot = state
        .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), &mut file)
        .await
        .unwrap();

    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.cued_scene_id, None);
}

#[test]
fn show_file_structural_round_trip_test() {
    let original = ShowFile {
        schema_version: 1,
        app_version: "0.1.0".to_string(),
        saved_at: "2026-06-10T12:00:00Z".to_string(),
        safety: crate::show_file::ShowFileSafety { lockout: true },
        cued_scene_id: None,
        scene_configs: vec![
            ShowFileSceneConfig {
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms: 5000,
                channel_configs: vec![
                    ShowFileChannelConfig {
                        group: 0,
                        channel: 1,
                        fader_db: Some(-12.5),
                        pan: Some(-15.0),
                        balance: Some(2.5),
                        width: Some(0.8),
                        pan_mode: Some(crate::lv1::types::PanMode::Stereo),
                    },
                    ShowFileChannelConfig {
                        group: 1,
                        channel: 3,
                        fader_db: Some(-6.0),
                        pan: None,
                        balance: None,
                        width: None,
                        pan_mode: None,
                    },
                ],
                scoped_channels: vec![
                    ShowFileChannelRef {
                        group: 0,
                        channel: 1,
                    },
                    ShowFileChannelRef {
                        group: 1,
                        channel: 3,
                    },
                ],
                scope_toggles: crate::show_file::ShowFileSceneScopeToggles {
                    faders: true,
                    pan: true,
                },
            },
            ShowFileSceneConfig {
                scene_index: 2,
                scene_name: "Verse".to_string(),
                duration_ms: 10000,
                channel_configs: vec![ShowFileChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-9.0),
                    pan: Some(5.5),
                    balance: Some(-1.0),
                    width: Some(1.5),
                    pan_mode: Some(crate::lv1::types::PanMode::Mono),
                }],
                scoped_channels: vec![ShowFileChannelRef {
                    group: 0,
                    channel: 2,
                }],
                scope_toggles: crate::show_file::ShowFileSceneScopeToggles {
                    faders: false,
                    pan: false,
                },
            },
        ],
    };

    let json = serde_json::to_string(&original).expect("Failed to serialize");
    let deserialized: ShowFile = serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(original, deserialized);
    assert_eq!(deserialized.schema_version, 1);
    assert_eq!(deserialized.app_version, "0.1.0");
    assert_eq!(deserialized.saved_at, "2026-06-10T12:00:00Z");
    assert!(deserialized.safety.lockout);
    assert_eq!(deserialized.scene_configs.len(), 2);

    let scene_1 = &deserialized.scene_configs[0];
    assert_eq!(scene_1.scene_index, 1);
    assert_eq!(scene_1.scene_name, "Intro");
    assert_eq!(scene_1.duration_ms, 5000);
    assert_eq!(scene_1.channel_configs.len(), 2);
    assert_eq!(scene_1.scoped_channels.len(), 2);
    assert!(scene_1.scope_toggles.faders);
    assert!(scene_1.scope_toggles.pan);

    let channel_1_1 = &scene_1.channel_configs[0];
    assert_eq!(channel_1_1.group, 0);
    assert_eq!(channel_1_1.channel, 1);
    assert_eq!(channel_1_1.fader_db, Some(-12.5));
    assert_eq!(channel_1_1.pan, Some(-15.0));
    assert_eq!(channel_1_1.balance, Some(2.5));
    assert_eq!(channel_1_1.width, Some(0.8));
    assert_eq!(
        channel_1_1.pan_mode,
        Some(crate::lv1::types::PanMode::Stereo)
    );

    let channel_1_2 = &scene_1.channel_configs[1];
    assert_eq!(channel_1_2.group, 1);
    assert_eq!(channel_1_2.channel, 3);
    assert_eq!(channel_1_2.fader_db, Some(-6.0));
    assert!(channel_1_2.pan.is_none());
    assert!(channel_1_2.balance.is_none());
    assert!(channel_1_2.width.is_none());
    assert!(channel_1_2.pan_mode.is_none());

    let scene_2 = &deserialized.scene_configs[1];
    assert_eq!(scene_2.scene_index, 2);
    assert_eq!(scene_2.scene_name, "Verse");
    assert_eq!(scene_2.duration_ms, 10000);
    assert_eq!(scene_2.channel_configs.len(), 1);
    assert_eq!(scene_2.scoped_channels.len(), 1);
    assert!(!scene_2.scope_toggles.faders);
    assert!(!scene_2.scope_toggles.pan);

    let channel_2_1 = &scene_2.channel_configs[0];
    assert_eq!(channel_2_1.group, 0);
    assert_eq!(channel_2_1.channel, 2);
    assert_eq!(channel_2_1.fader_db, Some(-9.0));
    assert_eq!(channel_2_1.pan, Some(5.5));
    assert_eq!(channel_2_1.balance, Some(-1.0));
    assert_eq!(channel_2_1.width, Some(1.5));
    assert_eq!(channel_2_1.pan_mode, Some(crate::lv1::types::PanMode::Mono));
}

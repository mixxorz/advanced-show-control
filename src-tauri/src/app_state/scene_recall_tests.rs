use lv1_scene_fade_utility::fade::curve::FadeCurve;
use lv1_scene_fade_utility::lv1::model::{
    ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneState,
};

use super::scene_recall::SceneRecallDecision;
use super::shell::ShellState;
use super::test_support::scene_config;
use super::view::{ChannelConfig, ChannelRef, LogSeverity, LogSource};

#[tokio::test]
async fn configured_nonzero_scene_builds_fade_request() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    {
        let mut inner = state.inner.lock().await;
        let mut config = scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef {
                group: 0,
                channel: 2,
            }],
        );
        config.duration_ms = 4_000;
        inner.scene_configs = vec![config];
    }

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    match decision {
        SceneRecallDecision::Start(request) => {
            assert_eq!(request.scene_id, "1::Intro");
            assert_eq!(request.scene_label, "1: Intro");
            assert_eq!(request.fade_config.duration_ms, 4_000);
            assert_eq!(request.fade_config.curve, FadeCurve::Linear);
            assert_eq!(request.fade_config.targets.len(), 1);
            assert_eq!(request.fade_config.targets[0].group, 0);
            assert_eq!(request.fade_config.targets[0].channel, 2);
            assert_eq!(request.fade_config.targets[0].target_db, -12.5);
        }
        other => panic!("unexpected decision: {other:?}"),
    }

    let snapshot = state.snapshot().await;
    assert!(snapshot.logs.iter().any(|log| {
        log.source == LogSource::App
            && log.severity == LogSeverity::Info
            && log.message == "Auto fade ready for scene 1: Intro with 1 target"
    }));
}

#[tokio::test]
async fn recalled_scene_overrides_stale_current_scene_snapshot() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    let mut snapshot = snapshot_for_intro();
    snapshot.scene = Some(SceneState {
        index: 0,
        name: "Old".to_string(),
    });
    state.begin_connection(snapshot).await;

    {
        let mut inner = state.inner.lock().await;
        let mut config = scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef {
                group: 0,
                channel: 2,
            }],
        );
        config.duration_ms = 4_000;
        inner.scene_configs = vec![config];
    }

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    match decision {
        SceneRecallDecision::Start(request) => {
            assert_eq!(request.scene_id, "1::Intro");
            assert_eq!(request.scene_label, "1: Intro");
        }
        other => panic!("unexpected decision: {other:?}"),
    }
}

#[tokio::test]
async fn duration_zero_skips_without_starting_fade() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    {
        let mut inner = state.inner.lock().await;
        inner.scene_configs = vec![scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef {
                group: 0,
                channel: 2,
            }],
        )];
    }

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Skip);
}

#[tokio::test]
async fn lockout_blocks_scene_recall_fade() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;
    state.set_lockout(true).await;

    {
        let mut inner = state.inner.lock().await;
        let mut config = scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef {
                group: 0,
                channel: 2,
            }],
        );
        config.duration_ms = 4_000;
        inner.scene_configs = vec![config];
    }

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Blocked);
}

#[tokio::test]
async fn missing_scene_config_skips() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Skip);
}

#[tokio::test]
async fn unconfigured_recalled_scene_skips_when_current_snapshot_is_stale() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Renamed Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Skip);
}

#[tokio::test]
async fn missing_live_channel_snapshot_blocks() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state
        .begin_connection(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(SceneState {
                index: 1,
                name: "Intro".to_string(),
            }),
            scene_list: Vec::new(),
            channels: Vec::new(),
        })
        .await;

    {
        let mut inner = state.inner.lock().await;
        let mut config = scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef {
                group: 0,
                channel: 2,
            }],
        );
        config.duration_ms = 4_000;
        inner.scene_configs = vec![config];
    }

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Blocked);
}

#[tokio::test]
async fn scoped_channel_without_stored_fader_value_blocks() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    {
        let mut inner = state.inner.lock().await;
        let mut config = scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: None,
            }],
            vec![ChannelRef {
                group: 0,
                channel: 2,
            }],
        );
        config.duration_ms = 4_000;
        inner.scene_configs = vec![config];
    }

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Blocked);
}

#[tokio::test]
async fn scoped_channel_missing_from_live_topology_blocks() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    {
        let mut inner = state.inner.lock().await;
        let mut config = scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 9,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef {
                group: 0,
                channel: 9,
            }],
        );
        config.duration_ms = 4_000;
        inner.scene_configs = vec![config];
    }

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::Blocked);
}

#[tokio::test]
async fn stale_generation_is_ignored() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    let (_next_generation, _) = state.disconnect().await;

    let decision = state
        .prepare_scene_recall_fade_for_generation(
            generation,
            &SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        )
        .await;

    assert_eq!(decision, SceneRecallDecision::StaleGeneration);
}

#[tokio::test]
async fn duration_zero_skip_logs_once_per_generation_for_same_scene() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    {
        let mut inner = state.inner.lock().await;
        inner.scene_configs = vec![scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef {
                group: 0,
                channel: 2,
            }],
        )];
    }

    for _ in 0..2 {
        assert_eq!(
            state
                .prepare_scene_recall_fade_for_generation(
                    generation,
                    &SceneState {
                        index: 1,
                        name: "Intro".to_string(),
                    },
                )
                .await,
            SceneRecallDecision::Skip
        );
    }

    let snapshot = state.snapshot().await;
    let skip_logs = snapshot
        .logs
        .iter()
        .filter(|log| log.message == "Auto fade skipped for scene 1: Intro: duration is 0")
        .count();
    assert_eq!(skip_logs, 1);
}

#[tokio::test]
async fn duration_zero_skip_logs_again_after_generation_changes() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    {
        let mut inner = state.inner.lock().await;
        inner.scene_configs = vec![scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef {
                group: 0,
                channel: 2,
            }],
        )];
    }

    assert_eq!(
        state
            .prepare_scene_recall_fade_for_generation(
                generation,
                &SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                },
            )
            .await,
        SceneRecallDecision::Skip
    );

    let (next_generation, _) = state.begin_connecting().await;
    state.begin_connection(snapshot_for_intro()).await;

    {
        let mut inner = state.inner.lock().await;
        inner.scene_configs = vec![scene_config(
            1,
            "Intro",
            vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-12.5),
            }],
            vec![ChannelRef {
                group: 0,
                channel: 2,
            }],
        )];
    }

    for _ in 0..2 {
        assert_eq!(
            state
                .prepare_scene_recall_fade_for_generation(
                    next_generation,
                    &SceneState {
                        index: 1,
                        name: "Intro".to_string(),
                    },
                )
                .await,
            SceneRecallDecision::Skip
        );
    }

    let snapshot = state.snapshot().await;
    let skip_logs = snapshot
        .logs
        .iter()
        .filter(|log| log.message == "Auto fade skipped for scene 1: Intro: duration is 0")
        .count();
    assert_eq!(skip_logs, 2);
}

fn snapshot_for_intro() -> Lv1StateSnapshot {
    Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: Some(SceneState {
            index: 1,
            name: "Intro".to_string(),
        }),
        scene_list: Vec::new(),
        channels: vec![ChannelInfo {
            group: 0,
            channel: 2,
            name: "Lead".to_string(),
            gain_db: -8.0,
            muted: false,
        }],
    }
}

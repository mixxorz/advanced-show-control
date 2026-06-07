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

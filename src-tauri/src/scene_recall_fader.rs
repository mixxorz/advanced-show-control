use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::runtime::commands::AppCommandBus;
use lv1_scene_fade_utility::runtime::events::{
    AppEvent, AppEventBus, AutomationEvent, log_lagged_subscriber,
};
use tokio::task::JoinHandle;

use crate::app_state::{SceneRecallDecision, ShellState};

pub fn spawn_scene_recall_fader(
    state: ShellState,
    generation: u64,
    command_bus: AppCommandBus,
    event_bus: AppEventBus,
) -> JoinHandle<()> {
    let mut events = event_bus.subscribe();

    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(AppEvent::Lv1(Lv1Event::SceneChanged(scene))) => {
                    if !state.is_generation_current(generation).await {
                        continue;
                    }

                    let snapshot = match command_bus.get_lv1_state().await {
                        Ok(snapshot) => snapshot,
                        Err(err) => {
                            if state
                                .log_scene_recall_fader_warning_for_generation(
                                    generation,
                                    format!(
                                        "Auto fade blocked for scene {}: {}: failed to fetch LV1 state: {err}",
                                        scene.index, scene.name
                                    ),
                                )
                                .await
                            {
                                publish_log_refresh(&event_bus);
                            }
                            continue;
                        }
                    };

                    match state
                        .prepare_scene_recall_fade_with_lv1_snapshot_for_generation(
                            generation, &scene, snapshot,
                        )
                        .await
                    {
                        SceneRecallDecision::Start(request) => {
                            if !state.is_generation_current(generation).await {
                                continue;
                            }

                            if state
                                .log_scene_recall_fader_info_for_generation(
                                    generation,
                                    format!(
                                    "Previous fade abort requested before auto fade for scene {}",
                                    request.scene_label
                                    ),
                                )
                                .await
                            {
                                publish_log_refresh(&event_bus);
                            } else {
                                continue;
                            }

                            if let Err(err) = command_bus.abort_all_fades().await {
                                if state
                                    .log_scene_recall_fader_warning_for_generation(
                                        generation,
                                        format!(
                                        "Auto fade blocked for scene {}: failed to abort previous fade: {err}",
                                        request.scene_label
                                        ),
                                    )
                                    .await
                                {
                                    publish_log_refresh(&event_bus);
                                }
                                continue;
                            }

                            if !state.is_generation_current(generation).await {
                                continue;
                            }

                            if state
                                .log_scene_recall_fader_info_for_generation(
                                    generation,
                                    format!(
                                        "Auto fade start requested for scene {}",
                                        request.scene_label
                                    ),
                                )
                                .await
                            {
                                publish_log_refresh(&event_bus);
                            } else {
                                continue;
                            }

                            if let Err(err) = command_bus.start_fade(request.fade_config).await {
                                if state
                                    .log_scene_recall_fader_warning_for_generation(
                                        generation,
                                        format!(
                                            "Auto fade failed for scene {}: {err}",
                                            request.scene_label
                                        ),
                                    )
                                    .await
                                {
                                    publish_log_refresh(&event_bus);
                                }
                            }
                        }
                        decision @ (SceneRecallDecision::Skip
                        | SceneRecallDecision::Blocked
                        | SceneRecallDecision::StaleGeneration) => {
                            publish_refresh_after_scene_recall_decision(&event_bus, &decision);
                        }
                    }
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    log_lagged_subscriber("scene-recall-fader", count);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

fn publish_log_refresh(event_bus: &AppEventBus) {
    event_bus.publish(AppEvent::Automation(AutomationEvent::RuleTriggered {
        rule_id: "scene-recall-fader".to_string(),
    }));
}

fn publish_refresh_after_scene_recall_decision(
    event_bus: &AppEventBus,
    decision: &SceneRecallDecision,
) {
    match decision {
        SceneRecallDecision::Skip | SceneRecallDecision::Blocked => publish_log_refresh(event_bus),
        SceneRecallDecision::Start(_) | SceneRecallDecision::StaleGeneration => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lv1_scene_fade_utility::fade::curve::FadeCurve;
    use lv1_scene_fade_utility::fade::engine::FadeEngineHandle;
    use lv1_scene_fade_utility::fade::types::FadeCommand;
    use lv1_scene_fade_utility::lv1::model::{
        ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry, SceneState,
    };
    use lv1_scene_fade_utility::lv1::state::spawn_actor;
    use lv1_scene_fade_utility::osc::OscArg;
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::mpsc as std_mpsc;
    use std::time::Duration;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn valid_scene_recall_aborts_existing_fade_then_starts_new_fade() {
        let event_bus = AppEventBus::default();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let state = ShellState::default();
        let generation = configure_intro_recall(&state).await;
        let (fade_tx, mut fade_rx) = mpsc::channel(4);
        command_bus
            .set_fade(Some(FadeEngineHandle::new(fade_tx)))
            .await;

        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        command_bus.set_lv1(Some(lv1)).await;

        let handle =
            spawn_scene_recall_fader(state.clone(), generation, command_bus, event_bus.clone());
        release_lv1.send(()).unwrap();

        let abort = tokio::time::timeout(Duration::from_secs(2), fade_rx.recv())
            .await
            .expect("timed out waiting for AbortAll")
            .expect("fade command channel should be open");
        match abort {
            FadeCommand::AbortAll { reply } => {
                let _ = reply.send(Ok(()));
            }
            other => panic!("expected AbortAll before StartFade, got {other:?}"),
        }

        let start = tokio::time::timeout(Duration::from_secs(2), fade_rx.recv())
            .await
            .expect("timed out waiting for StartFade")
            .expect("fade command channel should be open");
        match start {
            FadeCommand::StartFade { config, reply } => {
                assert_eq!(config.duration_ms, 4_000);
                assert_eq!(config.curve, FadeCurve::Linear);
                assert_eq!(config.targets.len(), 1);
                assert_eq!(config.targets[0].group, 0);
                assert_eq!(config.targets[0].channel, 2);
                assert_eq!(config.targets[0].target_db, -12.5);
                let _ = reply.send(Ok(()));
            }
            other => panic!("expected StartFade after AbortAll, got {other:?}"),
        }

        wait_for_log(&state, "Auto fade start requested for scene 1: Intro").await;
        let snapshot = state.snapshot().await;
        assert!(snapshot.logs.iter().any(|log| {
            log.message == "Previous fade abort requested before auto fade for scene 1: Intro"
        }));
        assert!(
            snapshot
                .logs
                .iter()
                .any(|log| log.message == "Auto fade start requested for scene 1: Intro")
        );

        handle.abort();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn blocked_recall_does_not_abort_existing_fade() {
        let event_bus = AppEventBus::default();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let state = ShellState::default();
        let generation = configure_intro_recall(&state).await;
        state.set_lockout(true).await;
        let (fade_tx, mut fade_rx) = mpsc::channel(4);
        command_bus
            .set_fade(Some(FadeEngineHandle::new(fade_tx)))
            .await;

        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        command_bus.set_lv1(Some(lv1)).await;

        let handle =
            spawn_scene_recall_fader(state.clone(), generation, command_bus, event_bus.clone());
        release_lv1.send(()).unwrap();

        wait_for_log(
            &state,
            "Auto fade blocked for scene 1: Intro: lockout is enabled",
        )
        .await;
        tokio::time::timeout(Duration::from_millis(100), fade_rx.recv())
            .await
            .expect_err("blocked recall should not send fade commands");

        handle.abort();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn unavailable_lv1_state_blocks_before_abort_or_start() {
        let event_bus = AppEventBus::default();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let state = ShellState::default();
        let generation = configure_intro_recall(&state).await;

        let handle =
            spawn_scene_recall_fader(state.clone(), generation, command_bus, event_bus.clone());

        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        wait_for_log(
            &state,
            "Auto fade blocked for scene 1: Intro: failed to fetch LV1 state: LV1 actor is unavailable",
        )
        .await;

        let snapshot = state.snapshot().await;
        assert!(
            !snapshot
                .logs
                .iter()
                .any(|log| log.message.contains("Previous fade abort requested"))
        );
        assert!(
            !snapshot
                .logs
                .iter()
                .any(|log| log.message.contains("Auto fade start requested"))
        );

        handle.abort();
    }

    #[tokio::test]
    async fn fader_log_writes_publish_automation_refresh() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let state = ShellState::default();
        let generation = configure_intro_recall(&state).await;

        let handle =
            spawn_scene_recall_fader(state.clone(), generation, command_bus, event_bus.clone());

        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        tokio::time::timeout(std::time::Duration::from_millis(250), async {
            loop {
                if let AppEvent::Automation(AutomationEvent::RuleTriggered { rule_id }) =
                    events.recv().await.unwrap()
                {
                    assert_eq!(rule_id, "scene-recall-fader");
                    break;
                }
            }
        })
        .await
        .expect("scene recall fader log should publish automation refresh");

        handle.abort();
    }

    #[tokio::test]
    async fn unavailable_lv1_state_does_not_log_start_request() {
        let event_bus = AppEventBus::default();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let state = ShellState::default();
        let generation = configure_intro_recall(&state).await;

        let handle =
            spawn_scene_recall_fader(state.clone(), generation, command_bus, event_bus.clone());

        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        wait_for_log(
            &state,
            "Auto fade blocked for scene 1: Intro: failed to fetch LV1 state: LV1 actor is unavailable",
        )
        .await;

        let snapshot = state.snapshot().await;
        assert!(
            !snapshot
                .logs
                .iter()
                .any(|log| log.message == "Auto fade start requested for scene 1: Intro")
        );

        handle.abort();
    }

    #[tokio::test]
    async fn blocked_or_skip_decision_publishes_automation_refresh() {
        for decision in [SceneRecallDecision::Blocked, SceneRecallDecision::Skip] {
            let event_bus = AppEventBus::default();
            let mut events = event_bus.subscribe();

            publish_refresh_after_scene_recall_decision(&event_bus, &decision);

            match tokio::time::timeout(std::time::Duration::from_millis(250), events.recv())
                .await
                .expect("non-stale decision should publish refresh")
                .expect("event bus should be open")
            {
                AppEvent::Automation(AutomationEvent::RuleTriggered { rule_id }) => {
                    assert_eq!(rule_id, "scene-recall-fader");
                }
                other => panic!("unexpected event: {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn stale_generation_decision_does_not_publish_automation_refresh() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();

        publish_refresh_after_scene_recall_decision(
            &event_bus,
            &SceneRecallDecision::StaleGeneration,
        );

        tokio::time::timeout(std::time::Duration::from_millis(50), events.recv())
            .await
            .expect_err("stale generation should not publish refresh");
    }

    async fn configure_intro_recall(state: &ShellState) -> u64 {
        let (generation, _) = state.begin_connecting().await;
        state.begin_connection(snapshot_for_intro()).await;
        state
            .store_scene_config("1::Intro".to_string())
            .await
            .unwrap();
        state
            .set_scene_duration_ms("1::Intro".to_string(), 4_000)
            .await
            .unwrap();
        generation
    }

    async fn spawn_fake_lv1_with_intro(
        event_bus: AppEventBus,
    ) -> (
        lv1_scene_fade_utility::lv1::state::Lv1ActorHandle,
        std_mpsc::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (release_tx, release_rx) = std_mpsc::channel();
        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            release_rx.recv().unwrap();
            stream.write_all(&channels_frame()).unwrap();
            stream.write_all(&scene_index_frame()).unwrap();
            stream.write_all(&scene_name_frame()).unwrap();
            std::thread::sleep(Duration::from_millis(250));
        });

        let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus);
        (lv1, release_tx, server)
    }

    fn channels_frame() -> Vec<u8> {
        let mut args = vec![OscArg::Int(1)];
        args.push(OscArg::String("Lead".to_string()));
        args.push(OscArg::Int(0));
        args.push(OscArg::Int(2));
        args.push(OscArg::Double(-8.0));
        for _ in 0..15 {
            args.push(OscArg::Int(0));
        }
        lv1_scene_fade_utility::lv1::tcp::encode_frame("/Channels", &args).unwrap()
    }

    fn scene_index_frame() -> Vec<u8> {
        lv1_scene_fade_utility::lv1::tcp::encode_frame("/Notify/CurSceneIndex", &[OscArg::Int(1)])
            .unwrap()
    }

    fn scene_name_frame() -> Vec<u8> {
        lv1_scene_fade_utility::lv1::tcp::encode_frame(
            "/Notify/Scene/Name",
            &[OscArg::String("Intro".to_string())],
        )
        .unwrap()
    }

    async fn wait_for_log(state: &ShellState, message: &str) {
        tokio::time::timeout(std::time::Duration::from_millis(250), async {
            loop {
                let snapshot = state.snapshot().await;
                if snapshot.logs.iter().any(|log| log.message == message) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for log: {message}"));
    }

    fn snapshot_for_intro() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(intro_scene()),
            scene_list: vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }],
            channels: vec![ChannelInfo {
                group: 0,
                channel: 2,
                name: "Lead".to_string(),
                gain_db: -12.5,
                muted: false,
            }],
        }
    }

    fn intro_scene() -> SceneState {
        SceneState {
            index: 1,
            name: "Intro".to_string(),
        }
    }
}

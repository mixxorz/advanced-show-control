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
                    match state
                        .prepare_scene_recall_fade_for_generation(generation, &scene)
                        .await
                    {
                        SceneRecallDecision::Start(request) => {
                            state
                                .log_scene_recall_fader_info(format!(
                                    "Previous fade abort requested before auto fade for scene {}",
                                    request.scene_label
                                ))
                                .await;
                            publish_log_refresh(&event_bus);

                            if let Err(err) = command_bus.abort_all_fades().await {
                                state
                                    .log_scene_recall_fader_warning(format!(
                                        "Auto fade blocked for scene {}: failed to abort previous fade: {err}",
                                        request.scene_label
                                    ))
                                    .await;
                                publish_log_refresh(&event_bus);
                                continue;
                            }

                            state
                                .log_scene_recall_fader_info(format!(
                                    "Auto fade start requested for scene {}",
                                    request.scene_label
                                ))
                                .await;
                            publish_log_refresh(&event_bus);

                            if let Err(err) = command_bus.start_fade(request.fade_config).await {
                                state
                                    .log_scene_recall_fader_warning(format!(
                                        "Auto fade failed for scene {}: {err}",
                                        request.scene_label
                                    ))
                                    .await;
                                publish_log_refresh(&event_bus);
                            }
                        }
                        SceneRecallDecision::Skip
                        | SceneRecallDecision::Blocked
                        | SceneRecallDecision::StaleGeneration => {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use lv1_scene_fade_utility::fade::engine::spawn_engine;
    use lv1_scene_fade_utility::lv1::model::{
        ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry, SceneState,
    };

    #[tokio::test]
    async fn abort_failure_blocks_start_and_logs_warning() {
        let event_bus = AppEventBus::default();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let state = ShellState::default();
        let generation = configure_intro_recall(&state).await;

        let handle =
            spawn_scene_recall_fader(state.clone(), generation, command_bus, event_bus.clone());

        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        wait_for_log(
            &state,
            "Auto fade blocked for scene 1: Intro: failed to abort previous fade: fade engine is unavailable",
        )
        .await;

        let snapshot = state.snapshot().await;
        assert!(snapshot.logs.iter().any(|log| {
            log.message == "Previous fade abort requested before auto fade for scene 1: Intro"
        }));
        assert!(
            !snapshot
                .logs
                .iter()
                .any(|log| log.message == "Auto fade start requested for scene 1: Intro")
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
    async fn start_failure_logs_warning_after_start_request() {
        let event_bus = AppEventBus::default();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let fade_command_bus = AppCommandBus::new(event_bus.clone());
        let fade = spawn_engine(fade_command_bus, event_bus.clone());
        command_bus.set_fade(Some(fade)).await;
        let state = ShellState::default();
        let generation = configure_intro_recall(&state).await;

        let handle =
            spawn_scene_recall_fader(state.clone(), generation, command_bus, event_bus.clone());

        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        wait_for_log(
            &state,
            "Auto fade failed for scene 1: Intro: LV1 actor is unavailable",
        )
        .await;

        let snapshot = state.snapshot().await;
        assert!(
            snapshot
                .logs
                .iter()
                .any(|log| log.message == "Auto fade start requested for scene 1: Intro")
        );

        handle.abort();
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

use std::collections::HashSet;

use crate::lv1::events::Lv1Event;
use crate::runtime::commands::AppCommandBus;
use crate::runtime::events::{AppEvent, AppEventBus, AutomationEvent, log_lagged_subscriber};
use crate::scene_recall::policy::{decide_scene_recall, RecallPolicyDecision, RecallPolicyInput};
use crate::scene_recall::state::SceneRecallState;

const AUTOMATION_RULE_ID: &str = "scene-recall-fader";

pub fn spawn_scene_recall_fader(
    generation: u64,
    command_bus: AppCommandBus,
    event_bus: AppEventBus,
) -> tokio::task::JoinHandle<()> {
    let mut events = event_bus.subscribe();

    tokio::spawn(async move {
        let mut recall_state = SceneRecallState::default();
        let mut duration_zero_logged: HashSet<String> = HashSet::new();

        loop {
            match events.recv().await {
                Ok(AppEvent::Lv1(Lv1Event::SceneChanged(scene))) => {
                    if !recall_state.accepts(&scene) {
                        continue;
                    }

                    if command_bus.get_generation().await != generation {
                        continue;
                    }

                    let lv1_snapshot = match fresh_lv1_snapshot(&command_bus, &scene).await {
                        Ok(snapshot) => snapshot,
                        Err(err) => {
                            event_bus.publish(AppEvent::SceneRecall(
                                crate::scene_recall::events::SceneRecallEvent::Blocked {
                                    scene_label: scene_label(&scene),
                                    reason: format!("LV1 state is unavailable: {err}"),
                                },
                            ));
                            continue;
                        }
                    };

                    let scene_id = format!("{}::{}", scene.index, scene.name);
                    let scene_config = match command_bus.get_scene_config(scene_id.clone()).await {
                        Ok(scene_config) => scene_config,
                        Err(err) => {
                            event_bus.publish(AppEvent::SceneRecall(
                                crate::scene_recall::events::SceneRecallEvent::Blocked {
                                    scene_label: scene_label(&scene),
                                    reason: format!("failed to fetch scene config: {err}"),
                                },
                            ));
                            continue;
                        }
                    };
                    let lockout = match command_bus.get_lockout().await {
                        Ok(lockout) => lockout,
                        Err(err) => {
                            event_bus.publish(AppEvent::SceneRecall(
                                crate::scene_recall::events::SceneRecallEvent::Blocked {
                                    scene_label: scene_label(&scene),
                                    reason: format!("failed to fetch lockout: {err}"),
                                },
                            ));
                            continue;
                        }
                    };

                    match decide_scene_recall(RecallPolicyInput {
                        recalled_scene: scene.clone(),
                        lv1_snapshot,
                        lockout,
                        scene_config,
                    }) {
                        RecallPolicyDecision::Start(fade_config) => {
                            let scene_label = scene_label(&scene);
                            if command_bus.get_generation().await != generation {
                                continue;
                            }
                            event_bus.publish(AppEvent::SceneRecall(
                                crate::scene_recall::events::SceneRecallEvent::Ready {
                                    scene_label: scene_label.clone(),
                                    target_count: fade_config.targets.len(),
                                },
                            ));
                            event_bus.publish(AppEvent::SceneRecall(
                                crate::scene_recall::events::SceneRecallEvent::StartRequested {
                                    scene_label: scene_label.clone(),
                                },
                            ));
                            event_bus.publish(AppEvent::Automation(AutomationEvent::RuleTriggered {
                                rule_id: AUTOMATION_RULE_ID.to_string(),
                            }));

                            if command_bus.get_generation().await != generation {
                                continue;
                            }
                            if command_bus.start_fade(fade_config).await.is_err() {
                                event_bus.publish(AppEvent::SceneRecall(
                                    crate::scene_recall::events::SceneRecallEvent::Blocked {
                                        scene_label,
                                        reason: "failed to start fade".to_string(),
                                    },
                                ));
                            }
                        }
                        RecallPolicyDecision::Skip { reason } => {
                            if reason == "duration is 0" && duration_zero_logged.insert(scene_id) {
                                event_bus.publish(AppEvent::SceneRecall(
                                    crate::scene_recall::events::SceneRecallEvent::Skipped {
                                        scene_label: scene_label(&scene),
                                        reason,
                                    },
                                ));
                            } else {
                                event_bus.publish(AppEvent::SceneRecall(
                                    crate::scene_recall::events::SceneRecallEvent::Skipped {
                                        scene_label: scene_label(&scene),
                                        reason,
                                    },
                                ));
                            }
                        }
                        RecallPolicyDecision::Blocked { reason } => {
                            event_bus.publish(AppEvent::SceneRecall(
                                crate::scene_recall::events::SceneRecallEvent::Blocked {
                                    scene_label: scene_label(&scene),
                                    reason,
                                },
                            ));
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

fn scene_label(scene: &crate::lv1::types::SceneState) -> String {
    format!("{}: {}", scene.index, scene.name)
}

async fn fresh_lv1_snapshot(
    command_bus: &AppCommandBus,
    scene: &crate::lv1::types::SceneState,
) -> Result<crate::lv1::types::Lv1StateSnapshot, crate::runtime::commands::AppCommandError> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let snapshot = command_bus.get_lv1_state().await?;
        if snapshot.connection == crate::lv1::types::ConnectionStatus::Connected
            && snapshot.scene.as_ref() == Some(scene)
            || tokio::time::Instant::now() >= deadline
        {
            return Ok(snapshot);
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::actor::spawn_actor;
    use crate::lv1::events::Lv1Event;
    use crate::lv1::types::SceneState;
    use crate::osc::OscArg;
    use crate::show::actor::spawn_show_state;
    use crate::show::types::{ChannelConfig, ChannelRef, SceneConfig, ShowSnapshot};
    use crate::scene_recall::events::SceneRecallEvent;
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::mpsc as std_mpsc;
    use std::time::Duration;

    #[tokio::test]
    async fn unavailable_lv1_state_blocks_before_start() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let command_bus = AppCommandBus::new(event_bus.clone());
        command_bus.set_generation(1).await;

        let handle = spawn_scene_recall_fader(1, command_bus.clone(), event_bus.clone());
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
        tokio::time::sleep(Duration::from_millis(2_050)).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        match next_scene_recall_event(&mut events).await {
            SceneRecallEvent::Blocked { reason, .. } => {
                assert!(reason.contains("LV1 state is unavailable"));
            }
            other => panic!("unexpected event: {other:?}"),
        }

        handle.abort();
    }

    #[tokio::test]
    async fn blocked_recall_does_not_start_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let command_bus = AppCommandBus::new(event_bus.clone());
        command_bus.set_generation(1).await;
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        command_bus.set_lv1(Some(lv1)).await;
        command_bus.set_show(Some(show.clone())).await;
        seed_show(&show).await;

        let handle = spawn_scene_recall_fader(1, command_bus.clone(), event_bus.clone());
        release_lv1.send(()).unwrap();
        wait_for_scene_event(&mut events, intro_scene()).await;
        tokio::time::sleep(Duration::from_millis(2_050)).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        assert!(matches!(next_scene_recall_event(&mut events).await, SceneRecallEvent::Blocked { .. }));

        handle.abort();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn stale_generation_does_not_start_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let command_bus = AppCommandBus::new(event_bus.clone());
        command_bus.set_generation(1).await;
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        command_bus.set_lv1(Some(lv1)).await;
        command_bus.set_show(Some(show.clone())).await;
        seed_show(&show).await;

        let handle = spawn_scene_recall_fader(1, command_bus.clone(), event_bus.clone());
        release_lv1.send(()).unwrap();
        wait_for_scene_event(&mut events, intro_scene()).await;

        tokio::time::sleep(Duration::from_millis(2_050)).await;
        command_bus.set_generation(2).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        handle.abort();
        server.await.unwrap();
    }

    async fn wait_for_scene_event(events: &mut tokio::sync::broadcast::Receiver<AppEvent>, scene: SceneState) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let AppEvent::Lv1(Lv1Event::SceneChanged(current)) = events.recv().await.unwrap() {
                    if current == scene { break; }
                }
            }
        })
        .await
        .unwrap();
    }

    async fn next_scene_recall_event(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    ) -> SceneRecallEvent {
        loop {
            if let AppEvent::SceneRecall(event) = events.recv().await.unwrap() {
                break event;
            }
        }
    }

    async fn spawn_fake_lv1_with_intro(event_bus: AppEventBus) -> (crate::lv1::handle::Lv1ActorHandle, std_mpsc::Sender<()>, tokio::task::JoinHandle<()>) {
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
        (spawn_actor("127.0.0.1".to_string(), port, event_bus), release_tx, server)
    }

    fn show_handle() -> crate::show::handle::ShowStateHandle {
        spawn_show_state(AppEventBus::default())
    }

    async fn seed_show(handle: &crate::show::handle::ShowStateHandle) {
        let snapshot = ShowSnapshot {
            lockout: false,
            scene_configs: vec![SceneConfig {
                scene_id: "1::Intro".to_string(),
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms: 4_000,
                channel_configs: vec![ChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-12.5),
                }],
                scoped_channels: vec![ChannelRef { group: 0, channel: 2 }],
            }],
        };
        handle.replace_snapshot(snapshot).await.unwrap();
    }

    fn channels_frame() -> Vec<u8> { let mut args = vec![OscArg::Int(1)]; args.push(OscArg::String("Lead".to_string())); args.push(OscArg::Int(0)); args.push(OscArg::Int(2)); args.push(OscArg::Double(-8.0)); for _ in 0..15 { args.push(OscArg::Int(0)); } crate::lv1::tcp::encode_frame("/Channels", &args).unwrap() }
    fn scene_index_frame() -> Vec<u8> { crate::lv1::tcp::encode_frame("/Notify/CurSceneIndex", &[OscArg::Int(1)]).unwrap() }
    fn scene_name_frame() -> Vec<u8> { crate::lv1::tcp::encode_frame("/Notify/Scene/Name", &[OscArg::String("Intro".to_string())]).unwrap() }
    fn intro_scene() -> SceneState { SceneState { index: 1, name: "Intro".to_string() } }
}

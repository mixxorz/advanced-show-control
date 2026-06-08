use std::collections::HashSet;

use crate::lv1::events::Lv1Event;
use crate::runtime::commands::AppCommandBus;
use crate::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use crate::scene_recall::policy::{RecallPolicyDecision, RecallPolicyInput, decide_scene_recall};
use crate::scene_recall::state::SceneRecallState;

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
                            if command_bus.get_generation().await != generation {
                                continue;
                            }
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
                            if command_bus.get_generation().await != generation {
                                continue;
                            }
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
                            if command_bus.get_generation().await != generation {
                                continue;
                            }
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
                            if command_bus.get_generation().await != generation {
                                continue;
                            }
                            // Re-check immediately before sending the fade start so a stale task cannot move faders.
                            if command_bus.start_fade(fade_config).await.is_err() {
                                if command_bus.get_generation().await != generation {
                                    continue;
                                }
                                event_bus.publish(AppEvent::SceneRecall(
                                    crate::scene_recall::events::SceneRecallEvent::Blocked {
                                        scene_label,
                                        reason: "failed to start fade".to_string(),
                                    },
                                ));
                            }
                        }
                        RecallPolicyDecision::Skip { reason } => {
                            if command_bus.get_generation().await != generation {
                                continue;
                            }
                            if reason != "duration is 0" || duration_zero_logged.insert(scene_id) {
                                event_bus.publish(AppEvent::SceneRecall(
                                    crate::scene_recall::events::SceneRecallEvent::Skipped {
                                        scene_label: scene_label(&scene),
                                        reason,
                                    },
                                ));
                            }
                        }
                        RecallPolicyDecision::Blocked { reason } => {
                            if command_bus.get_generation().await != generation {
                                continue;
                            }
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
                    log_lagged_subscriber("scene-recall", count);
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
    use crate::fade::commands::FadeCommand;
    use crate::fade::curve::FadeCurve;
    use crate::fade::handle::FadeEngineHandle;
    use crate::fade::types::{FadeConfig, FadeSceneIdentity, FadeTarget};
    use crate::lv1::events::Lv1Event;
    use crate::lv1::handle::Lv1ActorHandle;
    use crate::lv1::types::{Lv1StateSnapshot, SceneState};
    use crate::scene_recall::events::SceneRecallEvent;
    use crate::show::actor::spawn_show_state;
    use crate::show::types::{ChannelConfig, ChannelRef, SceneConfig, ShowSnapshot};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

    async fn arm_recall_state(event_bus: &AppEventBus) {
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        tokio::task::yield_now().await;
    }

    #[tokio::test(start_paused = true)]
    async fn unavailable_lv1_state_blocks_before_start() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let command_bus = AppCommandBus::new(event_bus.clone());
        command_bus.set_generation(1).await;

        let handle = spawn_scene_recall_fader(1, command_bus.clone(), event_bus.clone());
        arm_recall_state(&event_bus).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        match next_scene_recall_event(&mut events).await {
            SceneRecallEvent::Blocked { reason, .. } => {
                assert!(reason.contains("LV1 state is unavailable"));
            }
            other => panic!("unexpected event: {other:?}"),
        }

        handle.abort();
    }

    #[tokio::test(start_paused = true)]
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
        arm_recall_state(&event_bus).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        assert!(next_blocked_scene_recall_event(&mut events).await);

        handle.abort();
        command_bus.set_lv1(None).await;
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn stale_generation_does_not_start_fade() {
        let event_bus = AppEventBus::default();
        let command_bus = AppCommandBus::new(event_bus.clone());
        command_bus.set_generation(1).await;
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        command_bus.set_lv1(Some(lv1)).await;
        command_bus.set_show(Some(show.clone())).await;
        seed_show(&show).await;

        let handle = spawn_scene_recall_fader(1, command_bus.clone(), event_bus.clone());
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;
        command_bus.set_generation(2).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        handle.abort();
        command_bus.set_lv1(None).await;
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn valid_recall_starts_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let command_bus = AppCommandBus::new(event_bus.clone());
        command_bus.set_generation(1).await;
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
        command_bus.set_lv1(Some(lv1)).await;
        command_bus.set_fade(Some(fade)).await;
        command_bus.set_show(Some(show.clone())).await;
        seed_show(&show).await;

        let handle = spawn_scene_recall_fader(1, command_bus.clone(), event_bus.clone());
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        let mut seen_ready = false;
        let mut seen_start_requested = false;
        for _ in 0..2 {
            match next_app_event(&mut events).await {
                AppEvent::SceneRecall(SceneRecallEvent::Ready { .. }) => seen_ready = true,
                AppEvent::SceneRecall(SceneRecallEvent::StartRequested { .. }) => {
                    seen_start_requested = true
                }
                other => panic!("unexpected event: {other:?}"),
            }
        }
        assert!(seen_ready && seen_start_requested);

        let fade_command = tokio::time::timeout(Duration::from_secs(1), fade_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            fade_command.scene,
            FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string()
            }
        );
        assert_eq!(
            fade_command.targets,
            vec![FadeTarget {
                group: 0,
                channel: 2,
                target_db: -12.5
            }]
        );
        assert_eq!(fade_command.duration_ms, 4_000);
        assert!(matches!(fade_command.curve, FadeCurve::Linear));

        assert_eq!(fade_starts.load(Ordering::SeqCst), 1);

        handle.abort();
        command_bus.set_lv1(None).await;
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn arming_and_repeat_behavior() {
        let mut state = SceneRecallState::default();
        let scene = intro_scene();

        assert!(!state.accepts(&scene));
        assert!(!state.accepts(&scene));
        tokio::time::advance(Duration::from_secs(2)).await;
        assert!(state.accepts(&scene));
        assert!(!state.accepts(&scene));
        tokio::time::advance(Duration::from_millis(500)).await;
        assert!(state.accepts(&scene));
    }

    #[tokio::test(start_paused = true)]
    async fn skipped_recall_does_not_abort_existing_fade() {
        let event_bus = AppEventBus::default();
        let command_bus = AppCommandBus::new(event_bus.clone());
        command_bus.set_generation(1).await;
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
        command_bus.set_lv1(Some(lv1)).await;
        command_bus.set_fade(Some(fade)).await;
        command_bus.set_show(Some(show.clone())).await;
        seed_show_with_duration(&show, 0).await;

        let handle = spawn_scene_recall_fader(1, command_bus.clone(), event_bus.clone());
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;

        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
        tokio::task::yield_now().await;
        assert!(matches!(
            fade_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        assert_eq!(fade_starts.load(Ordering::SeqCst), 0);

        handle.abort();
        command_bus.set_lv1(None).await;
        server.await.unwrap();
    }

    async fn next_app_event(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> AppEvent {
        loop {
            let event = events.recv().await.unwrap();
            match event {
                AppEvent::SceneRecall(_) => return event,
                _ => continue,
            }
        }
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

    async fn next_blocked_scene_recall_event(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    ) -> bool {
        for _ in 0..3 {
            if matches!(
                next_scene_recall_event(events).await,
                SceneRecallEvent::Blocked { .. }
            ) {
                return true;
            }
        }
        false
    }

    async fn spawn_fake_lv1_with_intro(
        _event_bus: AppEventBus,
    ) -> (
        Lv1ActorHandle,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let _ = release_rx.await;
            let snapshot = Lv1StateSnapshot {
                connection: crate::lv1::types::ConnectionStatus::Connected,
                scene: Some(intro_scene()),
                scene_list: Vec::new(),
                channels: vec![crate::lv1::types::ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Lead".to_string(),
                    gain_db: -8.0,
                    muted: false,
                }],
            };
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    crate::lv1::commands::Lv1Command::GetState { reply } => {
                        let _ = reply.send(snapshot.clone());
                    }
                    crate::lv1::commands::Lv1Command::SetGain { reply, .. } => {
                        let _ = reply.send(Ok(()));
                    }
                    crate::lv1::commands::Lv1Command::SetMute { reply, .. } => {
                        let _ = reply.send(Ok(()));
                    }
                    crate::lv1::commands::Lv1Command::Flush { reply } => {
                        let _ = reply.send(Ok(()));
                    }
                }
            }
        });
        (
            crate::lv1::handle::Lv1ActorHandle::new(lv1_tx),
            release_tx,
            server,
        )
    }

    fn show_handle() -> crate::show::handle::ShowStateHandle {
        spawn_show_state(AppEventBus::default())
    }

    async fn seed_show(handle: &crate::show::handle::ShowStateHandle) {
        seed_show_with_duration(handle, 4_000).await;
    }

    async fn seed_show_with_duration(
        handle: &crate::show::handle::ShowStateHandle,
        duration_ms: u64,
    ) {
        let snapshot = ShowSnapshot {
            lockout: false,
            scene_configs: vec![SceneConfig {
                scene_id: "1::Intro".to_string(),
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms,
                channel_configs: vec![ChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-12.5),
                }],
                scoped_channels: vec![ChannelRef {
                    group: 0,
                    channel: 2,
                }],
            }],
        };
        handle.replace_snapshot(snapshot).await.unwrap();
    }

    fn fake_fade_handle() -> (
        FadeEngineHandle,
        tokio::sync::mpsc::Receiver<FadeConfig>,
        Arc<AtomicUsize>,
    ) {
        let (command_tx, mut command_rx) = tokio::sync::mpsc::channel(8);
        let (seen_tx, seen_rx) = tokio::sync::mpsc::channel(8);
        let starts = Arc::new(AtomicUsize::new(0));
        let starts_clone = starts.clone();
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                if let FadeCommand::RecallSceneFade { config, reply } = command {
                    let _ = seen_tx.send(config.clone()).await;
                    starts_clone.fetch_add(1, Ordering::SeqCst);
                    let _ = reply.send(Ok(()));
                }
            }
        });
        (FadeEngineHandle::new(command_tx), seen_rx, starts)
    }

    fn intro_scene() -> SceneState {
        SceneState {
            index: 1,
            name: "Intro".to_string(),
        }
    }
}

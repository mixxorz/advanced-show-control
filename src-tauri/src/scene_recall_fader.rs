use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::lv1::model::SceneState;
use lv1_scene_fade_utility::runtime::commands::AppCommandBus;
use lv1_scene_fade_utility::runtime::events::{
    AppEvent, AppEventBus, AutomationEvent, log_lagged_subscriber,
};
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};

use crate::app_state::{SceneRecallDecision, SceneRecallFadeRequest, ShellState};

const RECALL_ARMING_DELAY: Duration = Duration::from_millis(2_000);
const SAME_SCENE_REPEAT_DELAY: Duration = Duration::from_millis(500);

#[derive(Clone, PartialEq, Eq)]
struct RecallSceneIdentity {
    index: i32,
    name: String,
}

impl From<&SceneState> for RecallSceneIdentity {
    fn from(scene: &SceneState) -> Self {
        Self {
            index: scene.index,
            name: scene.name.clone(),
        }
    }
}

enum RecallTriggerGate {
    Priming {
        scene: Option<RecallSceneIdentity>,
        observed_at: Option<Instant>,
    },
    Armed {
        last_scene: Option<RecallSceneIdentity>,
        last_triggered_at: Option<Instant>,
    },
}

impl Default for RecallTriggerGate {
    fn default() -> Self {
        Self::Priming {
            scene: None,
            observed_at: None,
        }
    }
}

impl RecallTriggerGate {
    fn accepts(&mut self, current_scene: &SceneState) -> bool {
        let now = Instant::now();
        let scene_identity = RecallSceneIdentity::from(current_scene);

        match self {
            Self::Priming { scene, observed_at } => match observed_at {
                Some(first_observed_at)
                    if now.duration_since(*first_observed_at) >= RECALL_ARMING_DELAY =>
                {
                    let baseline_scene = scene.clone().unwrap_or_else(|| scene_identity.clone());
                    let baseline_at = *first_observed_at + RECALL_ARMING_DELAY;
                    *self = Self::Armed {
                        last_scene: Some(baseline_scene),
                        last_triggered_at: Some(baseline_at),
                    };
                    self.accepts(current_scene)
                }
                Some(_) => {
                    *scene = Some(scene_identity);
                    *observed_at = Some(now);
                    false
                }
                None => {
                    *scene = Some(scene_identity);
                    *observed_at = Some(now);
                    false
                }
            },
            Self::Armed {
                last_scene,
                last_triggered_at,
            } => {
                if last_scene.as_ref() == Some(&scene_identity)
                    && last_triggered_at
                        .map(|triggered_at| {
                            now.duration_since(triggered_at) < SAME_SCENE_REPEAT_DELAY
                        })
                        .unwrap_or(false)
                {
                    return false;
                }

                *last_scene = Some(scene_identity);
                *last_triggered_at = Some(now);
                true
            }
        }
    }
}

pub fn spawn_scene_recall_fader(
    state: ShellState,
    generation: u64,
    command_bus: AppCommandBus,
    event_bus: AppEventBus,
) -> JoinHandle<()> {
    let mut events = event_bus.subscribe();

    tokio::spawn(async move {
        let mut trigger_gate = RecallTriggerGate::default();

        loop {
            match events.recv().await {
                Ok(AppEvent::Lv1(Lv1Event::SceneChanged(scene))) => {
                    if !state.is_generation_current(generation).await {
                        continue;
                    }

                    if !trigger_gate.accepts(&scene) {
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
                            start_scene_recall_fade(
                                &state,
                                generation,
                                &command_bus,
                                &event_bus,
                                request,
                            )
                            .await;
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

async fn start_scene_recall_fade(
    state: &ShellState,
    generation: u64,
    command_bus: &AppCommandBus,
    event_bus: &AppEventBus,
    request: SceneRecallFadeRequest,
) {
    start_scene_recall_fade_with_hook(
        state,
        generation,
        command_bus,
        event_bus,
        request,
        || async {},
    )
    .await;
}

async fn start_scene_recall_fade_with_hook<F, Fut>(
    state: &ShellState,
    generation: u64,
    command_bus: &AppCommandBus,
    event_bus: &AppEventBus,
    request: SceneRecallFadeRequest,
    after_start_log: F,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    if !state.is_generation_current(generation).await {
        return;
    }

    if !state.is_generation_current(generation).await {
        return;
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
        publish_log_refresh(event_bus);
    } else {
        return;
    }

    after_start_log().await;

    if !state.is_generation_current(generation).await {
        return;
    }

    if let Err(err) = command_bus.start_fade(request.fade_config).await {
        if state
            .log_scene_recall_fader_warning_for_generation(
                generation,
                format!("Auto fade failed for scene {}: {err}", request.scene_label),
            )
            .await
        {
            publish_log_refresh(event_bus);
        }
    }
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

    #[tokio::test(start_paused = true)]
    async fn valid_scene_recall_starts_scene_fade_without_global_abort() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
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

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let AppEvent::Lv1(Lv1Event::SceneChanged(scene)) = events.recv().await.unwrap() {
                    if scene == intro_scene() {
                        break;
                    }
                }
            }
        })
        .await
        .expect("timed out waiting for initial scene sync");

        tokio::time::advance(RECALL_ARMING_DELAY + SAME_SCENE_REPEAT_DELAY).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        let start = tokio::time::timeout(Duration::from_secs(2), fade_rx.recv())
            .await
            .expect("timed out waiting for RecallSceneFade")
            .expect("fade command channel should be open");
        match start {
            FadeCommand::RecallSceneFade { config, reply } => {
                assert_eq!(config.duration_ms, 4_000);
                assert_eq!(config.curve, FadeCurve::Linear);
                assert_eq!(config.scene.index, 1);
                assert_eq!(config.scene.name, "Intro");
                assert_eq!(config.targets.len(), 1);
                assert_eq!(config.targets[0].group, 0);
                assert_eq!(config.targets[0].channel, 2);
                assert_eq!(config.targets[0].target_db, -12.5);
                let _ = reply.send(Ok(()));
            }
            other => panic!("expected RecallSceneFade without preceding AbortAll, got {other:?}"),
        }

        wait_for_log(&state, "Auto fade start requested for scene 1: Intro").await;
        let snapshot = state.snapshot().await;
        assert!(!snapshot.logs.iter().any(|log| {
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

    #[tokio::test(start_paused = true)]
    async fn duplicate_same_scene_notifications_inside_repeat_delay_send_one_fade_command() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
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
        wait_for_intro_scene_event(&mut events).await;

        tokio::time::advance(RECALL_ARMING_DELAY + SAME_SCENE_REPEAT_DELAY).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
        tokio::time::advance(Duration::from_millis(100)).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
        tokio::time::advance(Duration::from_millis(100)).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        let start = tokio::time::timeout(Duration::from_secs(2), fade_rx.recv())
            .await
            .expect("first actionable recall should send a fade command")
            .expect("fade command channel should be open");
        match start {
            FadeCommand::RecallSceneFade { reply, .. } => {
                let _ = reply.send(Ok(()));
            }
            other => panic!("expected RecallSceneFade, got {other:?}"),
        }

        tokio::task::yield_now().await;
        tokio::time::timeout(Duration::from_millis(1), fade_rx.recv())
            .await
            .expect_err("duplicate same-scene notifications should be suppressed");

        handle.abort();
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn reconnect_generation_primes_again_without_carrying_repeat_history() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let state = ShellState::default();
        let first_generation = configure_intro_recall(&state).await;
        let (fade_tx, mut fade_rx) = mpsc::channel(4);
        command_bus
            .set_fade(Some(FadeEngineHandle::new(fade_tx)))
            .await;

        let (lv1, release_lv1, first_server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        command_bus.set_lv1(Some(lv1)).await;

        let first_handle = spawn_scene_recall_fader(
            state.clone(),
            first_generation,
            command_bus.clone(),
            event_bus.clone(),
        );
        release_lv1.send(()).unwrap();
        wait_for_intro_scene_event(&mut events).await;

        tokio::time::advance(RECALL_ARMING_DELAY + SAME_SCENE_REPEAT_DELAY).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        let first = tokio::time::timeout(Duration::from_secs(2), fade_rx.recv())
            .await
            .expect("first generation recall should send a fade command")
            .expect("fade command channel should be open");
        match first {
            FadeCommand::RecallSceneFade { reply, .. } => {
                let _ = reply.send(Ok(()));
            }
            other => panic!("expected first RecallSceneFade, got {other:?}"),
        }

        let _ = state.disconnect().await;
        let (second_generation, _) = state.begin_connecting().await;
        state
            .begin_connection_for_generation(second_generation, snapshot_for_intro())
            .await
            .expect("second generation should accept reconnect snapshot");
        let mut second_events = event_bus.subscribe();
        let (second_lv1, release_second_lv1, close_second_lv1, second_server) =
            spawn_fake_lv1_with_intro_until_close(event_bus.clone()).await;
        command_bus.set_lv1(Some(second_lv1)).await;
        let second_handle = spawn_scene_recall_fader(
            state.clone(),
            second_generation,
            command_bus,
            event_bus.clone(),
        );
        release_second_lv1.send(()).unwrap();
        wait_for_intro_scene_event(&mut second_events).await;

        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
        assert!(
            matches!(fade_rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "new generation should prime before sending same-scene fade commands"
        );

        tokio::time::advance(RECALL_ARMING_DELAY + SAME_SCENE_REPEAT_DELAY).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        let second = tokio::time::timeout(Duration::from_secs(2), fade_rx.recv())
            .await
            .expect("second generation recall after priming should send a fade command")
            .expect("fade command channel should be open");
        match second {
            FadeCommand::RecallSceneFade { reply, .. } => {
                let _ = reply.send(Ok(()));
            }
            other => panic!("expected second RecallSceneFade, got {other:?}"),
        }

        first_handle.abort();
        second_handle.abort();
        close_second_lv1.send(()).unwrap();
        first_server.await.unwrap();
        second_server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn same_scene_repeat_after_repeat_delay_is_actionable() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
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
        wait_for_intro_scene_event(&mut events).await;

        tokio::time::advance(RECALL_ARMING_DELAY + SAME_SCENE_REPEAT_DELAY).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
        let first = tokio::time::timeout(Duration::from_secs(2), fade_rx.recv())
            .await
            .expect("first recall should send a fade command")
            .expect("fade command channel should be open");
        match first {
            FadeCommand::RecallSceneFade { reply, .. } => {
                let _ = reply.send(Ok(()));
            }
            other => panic!("expected first RecallSceneFade, got {other:?}"),
        }

        tokio::time::advance(SAME_SCENE_REPEAT_DELAY).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

        let second = tokio::time::timeout(Duration::from_secs(2), fade_rx.recv())
            .await
            .expect("same-scene repeat after delay should send a fade command")
            .expect("fade command channel should be open");
        match second {
            FadeCommand::RecallSceneFade { reply, .. } => {
                let _ = reply.send(Ok(()));
            }
            other => panic!("expected second RecallSceneFade, got {other:?}"),
        }

        handle.abort();
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn primed_same_scene_waits_for_repeat_delay_after_arming_boundary() {
        let mut gate = RecallTriggerGate::default();
        let scene = intro_scene();

        assert!(!gate.accepts(&scene));
        tokio::time::advance(RECALL_ARMING_DELAY).await;

        assert!(!gate.accepts(&scene));
        tokio::time::advance(SAME_SCENE_REPEAT_DELAY).await;

        assert!(gate.accepts(&scene));
    }

    #[tokio::test(start_paused = true)]
    async fn different_scene_after_arming_boundary_is_accepted_immediately() {
        let mut gate = RecallTriggerGate::default();
        let scene = intro_scene();
        let next_scene = SceneState {
            index: 2,
            name: "Verse".to_string(),
        };

        assert!(!gate.accepts(&scene));
        tokio::time::advance(RECALL_ARMING_DELAY).await;

        assert!(gate.accepts(&next_scene));
    }

    #[tokio::test(start_paused = true)]
    async fn latest_pre_arming_scene_observation_controls_repeat_delay() {
        let mut gate = RecallTriggerGate::default();
        let scene = intro_scene();
        let next_scene = SceneState {
            index: 2,
            name: "Verse".to_string(),
        };

        assert!(!gate.accepts(&scene));
        tokio::time::advance(Duration::from_millis(100)).await;

        assert!(!gate.accepts(&next_scene));
        tokio::time::advance(RECALL_ARMING_DELAY).await;

        assert!(!gate.accepts(&next_scene));
        tokio::time::advance(SAME_SCENE_REPEAT_DELAY).await;

        assert!(gate.accepts(&next_scene));
    }

    #[tokio::test(start_paused = true)]
    async fn first_scene_observation_after_connect_primes_without_starting_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
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

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let AppEvent::Lv1(Lv1Event::SceneChanged(scene)) = events.recv().await.unwrap() {
                    if scene == intro_scene() {
                        break;
                    }
                }
            }
        })
        .await
        .expect("timed out waiting for initial scene sync");

        tokio::time::timeout(Duration::from_millis(1), fade_rx.recv())
            .await
            .expect_err("initial scene sync should prime without sending fade commands");

        let snapshot = state.snapshot().await;
        assert!(
            !snapshot
                .logs
                .iter()
                .any(|log| log.message == "Auto fade start requested for scene 1: Intro")
        );

        handle.abort();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn stale_generation_after_start_log_does_not_start_fade() {
        let event_bus = AppEventBus::default();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let state = ShellState::default();
        let generation = configure_intro_recall(&state).await;
        let (fade_tx, mut fade_rx) = mpsc::channel(4);
        command_bus
            .set_fade(Some(FadeEngineHandle::new(fade_tx)))
            .await;
        let request = intro_fade_request();

        let fade_replies = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_millis(100), fade_rx.recv())
                .await
                .expect_err("stale generation should not send fade commands after start log");
        });

        start_scene_recall_fade_with_hook(
            &state,
            generation,
            &command_bus,
            &event_bus,
            request,
            || {
                let state = state.clone();
                async move {
                    let _ = state.disconnect().await;
                }
            },
        )
        .await;

        fade_replies.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn blocked_recall_does_not_abort_existing_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
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
        wait_for_intro_scene_event(&mut events).await;

        tokio::time::advance(RECALL_ARMING_DELAY + SAME_SCENE_REPEAT_DELAY).await;
        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

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

    #[tokio::test(start_paused = true)]
    async fn unavailable_lv1_state_blocks_before_abort_or_start() {
        let event_bus = AppEventBus::default();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let state = ShellState::default();
        let generation = configure_intro_recall(&state).await;

        let handle =
            spawn_scene_recall_fader(state.clone(), generation, command_bus, event_bus.clone());

        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
        tokio::task::yield_now().await;
        tokio::time::advance(RECALL_ARMING_DELAY + SAME_SCENE_REPEAT_DELAY).await;
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

    #[tokio::test(start_paused = true)]
    async fn fader_log_writes_publish_automation_refresh() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let state = ShellState::default();
        let generation = configure_intro_recall(&state).await;

        let handle =
            spawn_scene_recall_fader(state.clone(), generation, command_bus, event_bus.clone());

        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
        tokio::task::yield_now().await;
        tokio::time::advance(RECALL_ARMING_DELAY + SAME_SCENE_REPEAT_DELAY).await;
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

    #[tokio::test(start_paused = true)]
    async fn unavailable_lv1_state_does_not_log_start_request() {
        let event_bus = AppEventBus::default();
        let command_bus = AppCommandBus::new(event_bus.clone());
        let state = ShellState::default();
        let generation = configure_intro_recall(&state).await;

        let handle =
            spawn_scene_recall_fader(state.clone(), generation, command_bus, event_bus.clone());

        event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
        tokio::task::yield_now().await;
        tokio::time::advance(RECALL_ARMING_DELAY + SAME_SCENE_REPEAT_DELAY).await;
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

    async fn spawn_fake_lv1_with_intro_until_close(
        event_bus: AppEventBus,
    ) -> (
        lv1_scene_fade_utility::lv1::state::Lv1ActorHandle,
        std_mpsc::Sender<()>,
        std_mpsc::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (release_tx, release_rx) = std_mpsc::channel();
        let (close_tx, close_rx) = std_mpsc::channel();
        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            release_rx.recv().unwrap();
            stream.write_all(&channels_frame()).unwrap();
            stream.write_all(&scene_index_frame()).unwrap();
            stream.write_all(&scene_name_frame()).unwrap();
            let _ = close_rx.recv_timeout(Duration::from_secs(2));
        });

        let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus);
        (lv1, release_tx, close_tx, server)
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

    async fn wait_for_intro_scene_event(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    ) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let AppEvent::Lv1(Lv1Event::SceneChanged(scene)) = events.recv().await.unwrap() {
                    if scene == intro_scene() {
                        break;
                    }
                }
            }
        })
        .await
        .expect("timed out waiting for initial scene sync");
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

    fn intro_fade_request() -> SceneRecallFadeRequest {
        SceneRecallFadeRequest {
            scene_id: "1::Intro".to_string(),
            scene_label: "1: Intro".to_string(),
            fade_config: lv1_scene_fade_utility::fade::types::FadeConfig {
                scene: lv1_scene_fade_utility::fade::types::FadeSceneIdentity {
                    index: 1,
                    name: "Intro".to_string(),
                },
                targets: vec![lv1_scene_fade_utility::fade::types::FadeTarget {
                    group: 0,
                    channel: 2,
                    target_db: -12.5,
                }],
                duration_ms: 4_000,
                curve: FadeCurve::Linear,
            },
        }
    }
}

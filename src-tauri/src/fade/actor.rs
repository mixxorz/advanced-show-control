//! Fade engine actor — animates LV1 faders over time.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

use crate::fade::commands::FadeCommand;
use crate::fade::events::FadeEvent;
use crate::fade::handle::FadeEngineHandle;
use crate::fade::state::{EngineState, PingGateProgress, READINESS_PINGS_REQUIRED};
use crate::fade::tick::{ActiveTarget, ActiveTargetInit, TICK_HZ};
use crate::fade::types::{FadeParameter, FadeTarget};
use crate::lv1::{Lv1ActorHandle, Lv1Command, Lv1Event, Lv1ParameterWrite, Lv1WriteParameter};
use crate::runtime::errors::AppCommandError;
use crate::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use crate::runtime::generation::RuntimeGeneration;

#[derive(Clone, Default)]
pub struct FadeEnginePeers {
    lv1: Arc<Mutex<Option<Lv1ActorHandle>>>,
}

impl FadeEnginePeers {
    pub fn set_lv1(&self, lv1: Lv1ActorHandle) {
        *self.lv1.lock().expect("fade peer lock poisoned") = Some(lv1);
    }

    fn lv1(&self) -> Lv1ActorHandle {
        self.lv1
            .lock()
            .expect("fade peer lock poisoned")
            .clone()
            .expect("fade LV1 peer must be set before use")
    }
}

pub struct FadeEngineTask {
    runtime_generation: RuntimeGeneration,
    peers: FadeEnginePeers,
    event_bus: AppEventBus,
    generation: u64,
    cmd_rx: mpsc::Receiver<FadeCommand>,
}

impl FadeEngineTask {
    pub fn spawn(self) {
        tokio::spawn(run_engine(
            self.runtime_generation,
            self.peers,
            self.event_bus,
            self.generation,
            self.cmd_rx,
        ));
    }
}

pub fn build_engine(
    runtime_generation: RuntimeGeneration,
    event_bus: AppEventBus,
    generation: u64,
) -> (FadeEngineHandle, FadeEngineTask, FadeEnginePeers) {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    let handle = FadeEngineHandle::new(cmd_tx);
    let peers = FadeEnginePeers::default();
    let task = FadeEngineTask {
        runtime_generation,
        peers: peers.clone(),
        event_bus,
        generation,
        cmd_rx,
    };
    (handle, task, peers)
}

async fn run_engine(
    runtime_generation: RuntimeGeneration,
    peers: FadeEnginePeers,
    event_bus: AppEventBus,
    generation: u64,
    mut cmd_rx: mpsc::Receiver<FadeCommand>,
) {
    let mut app_events = event_bus.subscribe();
    let mut state = EngineState::new(event_bus.clone(), generation);
    let mut tick_interval: Option<tokio::time::Interval> = None;
    let mut fade_completed_emitted = false;

    loop {
        let tick_fut = async {
            match tick_interval.as_mut() {
                Some(interval) => {
                    interval.tick().await;
                    true
                }
                None => std::future::pending::<bool>().await,
            }
        };
        let readiness_deadline = state.readiness_deadline();
        let readiness_timeout_fut = async move {
            match readiness_deadline {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => std::future::pending::<()>().await,
            }
        };

        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    None => break,
                Some(FadeCommand::RecallSceneFade { config, expected_generation, reply }) => {
                        let scene_index = config.scene.index;
                        let scene_name = config.scene.name.clone();
                        let duration_ms = config.duration_ms;
                        let target_count = config.targets.len();
                        let lv1 = peers.lv1();
                        let result = handle_recall_scene_fade(&runtime_generation, &lv1, &mut state, config, expected_generation).await;

                        match result {
                            Ok(()) => {
                                if state.is_active() {
                                    let mut interval = tokio::time::interval(Duration::from_millis(1000 / TICK_HZ));
                                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                                    tick_interval = Some(interval);
                                    fade_completed_emitted = false;
                                    tracing::info!(event = "fade_started", scene_index = scene_index, scene_name = %scene_name, duration_ms = duration_ms, target_count = target_count, "Fade started for {}: {} ({} targets, {} ms)", scene_index, scene_name, target_count, duration_ms);
                                    state.fan_out(FadeEvent::FadeStarted);
                                } else {
                                    tick_interval = None;
                                    complete_fade(&mut tick_interval, &mut state, &mut fade_completed_emitted);
                                }
                                if let Some(reply) = reply {
                                    let _ = reply.send(Ok(()));
                                }
                            }
                            Err(err) => {
                                if let Some(reply) = reply {
                                    let _ = reply.send(Err(err));
                                }
                            }
                        }
                    }
                    Some(FadeCommand::AbortAll { reply }) => {
                        state.cancel_all_in_place();
                        tick_interval = None;
                        state.fan_out(FadeEvent::FadeAborted);
                        if let Some(reply) = reply {
                            let _ = reply.send(Ok(()));
                        }
                    }
                }
            }

            _ = tick_fut => {
                if state.is_waiting_for_readiness() {
                    continue;
                }

                let now = Instant::now();
                let mut done_indices = Vec::new();
                let mut completed_events = Vec::new();
                let mut writes = Vec::new();

                for (i, ch) in state.channels.iter_mut().enumerate() {
                    if ch.is_done(now) {
                        let target_db = ch.exact_final_send();
                        writes.push(build_parameter_write(ch.group, ch.channel, ch.key.parameter, target_db));
                        completed_events.push(FadeEvent::ChannelCompleted {
                            group: ch.group,
                            channel: ch.channel,
                            parameter: ch.key.parameter,
                        });
                        done_indices.push(i);
                        continue;
                    }

                    if let Some(new_value) = ch.next_send(now) {
                        writes.push(build_parameter_write(ch.group, ch.channel, ch.key.parameter, new_value));
                    }
                }

                if !writes.is_empty() {
                    for (expected_generation, writes) in group_writes_by_generation(&state.channels, writes) {
                        let sent = match expected_generation {
                            Some(expected_generation) => {
                                let lv1 = peers.lv1();
                                send_batch_if_generation(
                                    &runtime_generation,
                                    &lv1,
                                    &state.event_bus,
                                    expected_generation,
                                    writes,
                                )
                                .await
                            }
                            None => {
                                let lv1 = peers.lv1();
                                send_batch(&lv1, &state.event_bus, writes).await;
                                true
                            }
                        };

                        if !sent {
                            if let Some(expected_generation) = expected_generation {
                                cancel_generation_owned_targets(&mut state, expected_generation);
                            }
                            maybe_complete_fade(&mut tick_interval, &mut state, &mut fade_completed_emitted);
                        }
                    }
                }

                for i in done_indices.into_iter().rev() {
                    state.channels.remove(i);
                }

                for event in completed_events {
                    state.fan_out(event);
                }

                maybe_complete_fade(&mut tick_interval, &mut state, &mut fade_completed_emitted);
            }

            app_event = app_events.recv() => {
                match app_event {
                    Ok(AppEvent::Lv1 { event: Lv1Event::FaderChanged { group, channel, gain_db }, .. }) => {
                        if let Some(pos) = state.channels.iter().position(|ch| ch.group == group && ch.channel == channel && ch.key.parameter == FadeParameter::FaderDb)
                            && state.channels[pos].is_override(gain_db)
                        {
                            state.fan_out(FadeEvent::ChannelOverride {
                                group,
                                channel,
                                parameter: FadeParameter::FaderDb,
                            });
                            tracing::warn!(
                                event = "fade_manual_override",
                                group,
                                channel,
                                parameter = ?FadeParameter::FaderDb,
                                "Fade manual override detected: group {group}, channel {channel}"
                            );
                            state.channels.remove(pos);
                            state.fan_out(FadeEvent::ChannelCancelled {
                                group,
                                channel,
                                parameter: FadeParameter::FaderDb,
                            });

                            if !state.is_active() {
                                tick_interval = None;
                                fade_completed_emitted = false;
                                state.fan_out(FadeEvent::FadeCompleted);
                            }
                        }
                    }
                    Ok(AppEvent::Lv1 { event: Lv1Event::PanChanged { group, channel, pan }, .. }) => {
                        handle_pan_family_pan_report(
                            &mut state,
                            group,
                            channel,
                            pan,
                            &mut tick_interval,
                            &mut fade_completed_emitted,
                        );
                    }
                    Ok(AppEvent::Lv1 {
                        generation: event_generation,
                        event: Lv1Event::Disconnected { .. },
                    }) if event_generation == generation => {
                        if state.is_active() || state.is_waiting_for_readiness() {
                            state.cancel_all_in_place();
                            tick_interval = None;
                            fade_completed_emitted = false;
                            tracing::warn!(event = "fade_aborted", "Fade aborted");
                            state.fan_out(FadeEvent::FadeAborted);
                        }
                    }
                    Ok(AppEvent::Runtime(
                        crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged {
                            generation: event_generation,
                        },
                    )) if event_generation != generation => {
                        if state.is_active() || state.is_waiting_for_readiness() {
                            state.cancel_all_in_place();
                            tick_interval = None;
                            fade_completed_emitted = false;
                            state.fan_out(FadeEvent::FadeAborted);
                        }
                    }
                    Ok(AppEvent::Lv1 {
                        generation: event_generation,
                        event: Lv1Event::PingReceived { sequence },
                    }) => match state.observe_ping(event_generation, sequence, Instant::now()) {
                        PingGateProgress::Ignored => {}
                        PingGateProgress::Waiting { observed } => tracing::debug!(
                            event = "fade_post_recall_ping_waiting",
                            observed,
                            required = READINESS_PINGS_REQUIRED,
                            "Fade readiness is waiting for LV1 keepalive pings"
                        ),
                        PingGateProgress::Released => tracing::debug!(
                            event = "fade_post_recall_ping_released",
                            "Fade readiness released after LV1 keepalive resumed"
                        ),
                    },
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        log_lagged_subscriber("fade-engine", count);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }

            _ = readiness_timeout_fut => {
                if let Some(context) = state.readiness_timeout_context() {
                    state.cancel_all_in_place();
                    tick_interval = None;
                    fade_completed_emitted = false;
                    tracing::warn!(
                        event = "fade_post_recall_ping_timeout",
                        generation = context.generation,
                        scene_index = context.scene_index,
                        scene_name = %context.scene_name,
                        observed_ping_count = context.observed_ping_count,
                        timeout_ms = 5_000_u64,
                        "Fades were aborted because LV1 did not resume its keepalive cadence after scene recall"
                    );
                    state.fan_out(FadeEvent::FadeAborted);
                }
            }
        }
    }
}

fn maybe_complete_fade(
    tick_interval: &mut Option<tokio::time::Interval>,
    state: &mut EngineState,
    fade_completed_emitted: &mut bool,
) {
    if !state.is_active() {
        complete_fade(tick_interval, state, fade_completed_emitted);
    }
}

fn complete_fade(
    tick_interval: &mut Option<tokio::time::Interval>,
    state: &mut EngineState,
    fade_completed_emitted: &mut bool,
) {
    if *fade_completed_emitted {
        return;
    }
    *fade_completed_emitted = true;
    *tick_interval = None;
    tracing::info!(event = "fade_completed", "Fade completed");
    state.fan_out(FadeEvent::FadeCompleted);
}

async fn handle_recall_scene_fade(
    runtime_generation: &RuntimeGeneration,
    lv1: &Lv1ActorHandle,
    state: &mut EngineState,
    config: crate::fade::types::FadeConfig,
    expected_generation: Option<u64>,
) -> Result<(), AppCommandError> {
    if let Some(expected_generation) = expected_generation
        && runtime_generation.current().await != expected_generation
    {
        return Err(AppCommandError::StaleGeneration);
    }

    if config.targets.is_empty() {
        return Ok(());
    }

    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::GetState { reply })
        .await
        .map_err(|error| match error {
            crate::lv1::Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
            other => AppCommandError::CommandFailed(other.to_string()),
        })?;
    let snapshot = rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?;
    if let Some(expected_generation) = expected_generation
        && runtime_generation.current().await != expected_generation
    {
        return Err(AppCommandError::StaleGeneration);
    }

    let now = Instant::now();
    let duration = Duration::from_millis(config.duration_ms);

    if duration.is_zero() {
        let writes = config
            .targets
            .iter()
            .map(|target| {
                build_parameter_write(
                    target.group,
                    target.channel,
                    target.parameter,
                    target.target,
                )
            })
            .collect();
        if let Some(expected_generation) = expected_generation {
            if !send_batch_if_generation(
                runtime_generation,
                lv1,
                &state.event_bus,
                expected_generation,
                writes,
            )
            .await
            {
                return Err(AppCommandError::StaleGeneration);
            }
        } else {
            send_batch(lv1, &state.event_bus, writes).await;
        }

        for target in &config.targets {
            state.channels.retain(|ch| ch.key != target.key());
            state.fan_out(FadeEvent::ChannelCompleted {
                group: target.group,
                channel: target.channel,
                parameter: target.parameter,
            });
            tracing::debug!(event = "fade_channel_completed", group = target.group, channel = target.channel, parameter = ?target.parameter, "Fade channel completed: group {}, channel {}", target.group, target.channel);
        }
        return Ok(());
    }

    for target in &config.targets {
        let start_value = state
            .channels
            .iter()
            .find(|ch| ch.key == target.key())
            .map(|ch| {
                if ch.is_done(now) {
                    ch.target_value
                } else {
                    ch.value_at(now)
                }
            })
            .or_else(|| {
                snapshot
                    .channels
                    .iter()
                    .find(|ch| ch.group == target.group && ch.channel == target.channel)
                    .and_then(|ch| live_value_for_snapshot(ch, target))
            })
            .unwrap_or(target.target);

        state.channels.retain(|ch| ch.key != target.key());
        state.channels.push(ActiveTarget::new(ActiveTargetInit {
            key: target.key(),
            group: target.group,
            channel: target.channel,
            start_value,
            target_value: target.target,
            curve: config.curve,
            duration,
            started_at: now,
            expected_generation,
        }));
    }

    state.start_or_reset_readiness(
        expected_generation.unwrap_or(state.generation()),
        config.scene.index,
        config.scene.name,
        snapshot.ping_sequence,
        now,
        tokio::time::Instant::now(),
    );

    Ok(())
}

fn live_value_for_snapshot(channel: &crate::lv1::ChannelInfo, target: &FadeTarget) -> Option<f64> {
    match target.parameter {
        FadeParameter::FaderDb => Some(channel.gain_db),
        FadeParameter::Pan => channel.pan,
        FadeParameter::Balance => channel.balance,
        FadeParameter::Width => channel.width,
    }
}

fn build_parameter_write(
    group: i32,
    channel: i32,
    parameter: FadeParameter,
    value: f64,
) -> Lv1ParameterWrite {
    Lv1ParameterWrite {
        group,
        channel,
        parameter: match parameter {
            FadeParameter::FaderDb => Lv1WriteParameter::FaderDb,
            FadeParameter::Pan => Lv1WriteParameter::Pan,
            FadeParameter::Balance => Lv1WriteParameter::Balance,
            FadeParameter::Width => Lv1WriteParameter::Width,
        },
        value,
    }
}

async fn send_batch(lv1: &Lv1ActorHandle, event_bus: &AppEventBus, writes: Vec<Lv1ParameterWrite>) {
    if let Err(err) = lv1.send(Lv1Command::WriteBatch(writes)).await {
        let reason = format!("{err:?}");
        tracing::error!(event = "fade_write_failed", reason = %reason, "Fade write failed: {reason}");
        event_bus.publish(AppEvent::Fade {
            generation: 0,
            event: FadeEvent::WriteFailed { reason },
        });
    }
}

async fn send_batch_if_generation(
    runtime_generation: &RuntimeGeneration,
    lv1: &Lv1ActorHandle,
    event_bus: &AppEventBus,
    expected_generation: u64,
    writes: Vec<Lv1ParameterWrite>,
) -> bool {
    if runtime_generation.current().await != expected_generation {
        return false;
    }

    if let Err(err) = lv1.send(Lv1Command::WriteBatch(writes)).await {
        let reason = format!("{err:?}");
        tracing::error!(event = "fade_write_failed", reason = %reason, "Fade write failed: {reason}");
        event_bus.publish(AppEvent::Fade {
            generation: 0,
            event: FadeEvent::WriteFailed { reason },
        });
        return false;
    }

    true
}

fn group_writes_by_generation(
    channels: &[ActiveTarget],
    writes: Vec<Lv1ParameterWrite>,
) -> Vec<(Option<u64>, Vec<Lv1ParameterWrite>)> {
    let mut grouped: Vec<(Option<u64>, Vec<Lv1ParameterWrite>)> = Vec::new();

    for write in writes {
        let expected_generation = channels
            .iter()
            .find(|ch| ch.group == write.group && ch.channel == write.channel)
            .and_then(|ch| ch.expected_generation);
        if let Some((_, batch)) = grouped
            .iter_mut()
            .find(|(generation, _)| *generation == expected_generation)
        {
            batch.push(write);
        } else {
            grouped.push((expected_generation, vec![write]));
        }
    }

    grouped
}

fn cancel_generation_owned_targets(state: &mut EngineState, expected_generation: u64) {
    let mut removed = Vec::new();
    state.channels.retain(|ch| {
        let keep = ch.expected_generation != Some(expected_generation);
        if !keep {
            removed.push((ch.group, ch.channel, ch.key.parameter));
        }
        keep
    });
    for (group, channel, parameter) in removed {
        state.fan_out(FadeEvent::ChannelCancelled {
            group,
            channel,
            parameter,
        });
    }
}

fn handle_pan_family_pan_report(
    state: &mut EngineState,
    group: i32,
    channel: i32,
    reported_pan: f64,
    tick_interval: &mut Option<tokio::time::Interval>,
    fade_completed_emitted: &mut bool,
) {
    let pan_override = if let Some(pan_target) = state.channels.iter_mut().find(|ch| {
        ch.group == group && ch.channel == channel && ch.key.parameter == FadeParameter::Pan
    }) {
        // A single unexpected pan echo can be stale LV1 feedback during a reversal.
        // Wait for the configured number of consecutive misses before treating it
        // as an engineer grab.
        let expected_pan = pan_target.expected_value;
        let is_out_of_threshold = pan_target.is_override(reported_pan);
        let confirmed = pan_target.record_override_report(reported_pan);
        if is_out_of_threshold {
            tracing::debug!(
                event = "pan_override_suspect",
                group,
                channel,
                reported_pan,
                expected_pan,
                threshold = crate::fade::tick::PAN_OVERRIDE_THRESHOLD,
                confirmation_count = pan_target.override_deviation_count,
                required_confirmation_count = crate::fade::tick::PAN_OVERRIDE_CONFIRMATION_COUNT,
                "Pan override suspect: group {}, channel {}, reported {}, expected {}",
                group,
                channel,
                reported_pan,
                expected_pan
            );
        }
        confirmed
    } else {
        // If only balance/width are still fading, a pan move is still a manual
        // pan-family intervention. There is no active pan target to compare
        // against, so cancel those remaining targets immediately.
        state.channels.iter().any(|ch| {
            ch.group == group && ch.channel == channel && ch.key.parameter.is_pan_family()
        })
    };

    if !pan_override {
        return;
    }

    // Once the engineer grabs pan, stop the app's pan-family automation for
    // this channel. Leave the channel fader fade running; pan override should
    // not disrupt level automation.
    let mut removed = Vec::new();
    state.channels.retain(|ch| {
        let should_remove =
            ch.group == group && ch.channel == channel && ch.key.parameter.is_pan_family();
        if should_remove {
            removed.push(ch.key.parameter);
        }
        !should_remove
    });
    state.fan_out(FadeEvent::ChannelOverride {
        group,
        channel,
        parameter: FadeParameter::Pan,
    });
    // Report each stopped pan-family parameter so logs/UI show what the app
    // handed back to the engineer.
    for parameter in removed {
        state.fan_out(FadeEvent::ChannelCancelled {
            group,
            channel,
            parameter,
        });
    }

    if !state.is_active() {
        // If that was the last automated move, close out the fade cleanly.
        complete_fade(tick_interval, state, fade_completed_emitted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::curve::FadeCurve;
    use crate::fade::handle::FadeEngineHandle;
    use crate::fade::types::{FadeConfig, FadeSceneIdentity, FadeTarget};
    use crate::lv1::{
        ConnectionStatus, Lv1Command, Lv1Event, Lv1ParameterWrite, Lv1StateSnapshot,
        Lv1WriteParameter, test_actor_handle,
    };
    use crate::runtime::errors::AppCommandError;
    use crate::runtime::events::{AppEventBus, RuntimeLifecycleEvent};
    use std::sync::Arc;
    use tracing::field::{Field, Visit};
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::Context;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::registry::{LookupSpan, Registry};

    fn scene(index: i32, name: &str) -> FadeSceneIdentity {
        FadeSceneIdentity {
            index,
            name: name.to_string(),
        }
    }

    fn fade_config(
        scene: FadeSceneIdentity,
        targets: Vec<FadeTarget>,
        duration_ms: u64,
    ) -> FadeConfig {
        FadeConfig {
            scene,
            targets,
            duration_ms,
            curve: FadeCurve::Linear,
        }
    }

    fn active_pan_family_target(parameter: FadeParameter) -> ActiveTarget {
        let target = FadeTarget {
            group: 0,
            channel: 0,
            parameter,
            target: 45.0,
        };

        ActiveTarget::new(ActiveTargetInit {
            key: target.key(),
            group: target.group,
            channel: target.channel,
            start_value: 0.0,
            target_value: target.target,
            curve: FadeCurve::Linear,
            duration: std::time::Duration::from_millis(1000),
            started_at: Instant::now(),
            expected_generation: None,
        })
    }

    async fn spawn_runtime_for_test() -> (
        AppEventBus,
        FadeEngineHandle,
        tokio::sync::mpsc::Receiver<Lv1Command>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let event_bus = AppEventBus::default();
        let lv1 = test_actor_handle(tx);
        let runtime_generation = RuntimeGeneration::new();
        let (engine, task, peers) = build_engine(runtime_generation, event_bus.clone(), 0);
        peers.set_lv1(lv1);
        task.spawn();

        let mut events = event_bus.subscribe();
        let ping_bus = event_bus.clone();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(AppEvent::Fade {
                        generation,
                        event: FadeEvent::FadeStarted,
                    }) => {
                        // Existing actor tests do not model LV1's keepalive loop.
                        ping_bus.publish(AppEvent::Lv1 {
                            generation,
                            event: Lv1Event::PingReceived { sequence: 1 },
                        });
                        ping_bus.publish(AppEvent::Lv1 {
                            generation,
                            event: Lv1Event::PingReceived { sequence: 2 },
                        });
                    }
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        (event_bus, engine, rx)
    }

    async fn start_fade(
        engine: &FadeEngineHandle,
        config: FadeConfig,
    ) -> Result<(), AppCommandError> {
        start_fade_for_generation(engine, config, None).await
    }

    async fn start_fade_for_generation(
        engine: &FadeEngineHandle,
        config: FadeConfig,
        expected_generation: Option<u64>,
    ) -> Result<(), AppCommandError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        engine
            .send(FadeCommand::RecallSceneFade {
                config,
                expected_generation,
                reply: Some(reply),
            })
            .await
            .map_err(|_| AppCommandError::FadeUnavailable)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    fn connected_snapshot(
        ping_sequence: u64,
        channels: Vec<crate::lv1::ChannelInfo>,
    ) -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![],
            channels,
            ping_sequence,
        }
    }

    fn channel_info(channel: i32, gain_db: f64, pan: Option<f64>) -> crate::lv1::ChannelInfo {
        crate::lv1::ChannelInfo {
            group: 0,
            channel,
            name: format!("Channel {channel}"),
            gain_db,
            muted: false,
            pan,
            balance: None,
            width: None,
            pan_mode: None,
        }
    }

    async fn spawn_runtime_for_ping_gate_test(
        snapshots: Vec<Lv1StateSnapshot>,
    ) -> (
        AppEventBus,
        FadeEngineHandle,
        tokio::sync::mpsc::Receiver<Vec<Lv1ParameterWrite>>,
    ) {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let event_bus = AppEventBus::default();
        let lv1 = test_actor_handle(tx);
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(7).await;
        let (engine, task, peers) = build_engine(runtime_generation, event_bus.clone(), 7);
        peers.set_lv1(lv1);
        task.spawn();

        let (write_tx, write_rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            let mut snapshots = snapshots.into_iter();
            while let Some(command) = rx.recv().await {
                match command {
                    Lv1Command::GetState { reply } => {
                        let snapshot = snapshots.next().expect("unexpected LV1 state lookup");
                        let _ = reply.send(snapshot);
                    }
                    Lv1Command::WriteBatch(writes) => {
                        let _ = write_tx.send(writes).await;
                    }
                    _ => panic!("unexpected LV1 command"),
                }
            }
        });

        (event_bus, engine, write_rx)
    }

    async fn assert_no_write(write_rx: &mut tokio::sync::mpsc::Receiver<Vec<Lv1ParameterWrite>>) {
        assert!(
            tokio::time::timeout(Duration::from_millis(100), write_rx.recv())
                .await
                .is_err(),
            "fade write was sent before the readiness gate released"
        );
    }

    async fn assert_no_write_after_cancellation(
        write_rx: &mut tokio::sync::mpsc::Receiver<Vec<Lv1ParameterWrite>>,
    ) {
        tokio::task::yield_now().await;
        assert!(
            write_rx.try_recv().is_err(),
            "fade write was sent after readiness cancellation"
        );
    }

    async fn wait_for_fade_aborted(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
        loop {
            if let AppEvent::Fade {
                generation: 7,
                event: FadeEvent::FadeAborted,
            } = events.recv().await.expect("event bus should remain open")
            {
                return;
            }
        }
    }

    async fn assert_no_additional_fade_abort(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    ) {
        tokio::task::yield_now().await;
        while let Ok(event) = events.try_recv() {
            assert!(
                !matches!(
                    event,
                    AppEvent::Fade {
                        generation: 7,
                        event: FadeEvent::FadeAborted,
                    }
                ),
                "cancellation must emit only one FadeAborted event"
            );
        }
    }

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    struct CapturedWarnEvent {
        level: Option<String>,
        event: Option<String>,
        message: Option<String>,
        generation: Option<String>,
        scene_index: Option<String>,
        scene_name: Option<String>,
        observed_ping_count: Option<String>,
        timeout_ms: Option<String>,
    }

    #[derive(Clone, Default)]
    struct CapturedWarnEvents(Arc<std::sync::Mutex<Vec<CapturedWarnEvent>>>);

    impl<S> Layer<S> for CapturedWarnEvents
    where
        S: tracing::Subscriber,
        S: for<'a> LookupSpan<'a>,
    {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut captured = CapturedWarnEvent {
                level: Some(event.metadata().level().as_str().to_string()),
                ..Default::default()
            };
            event.record(&mut captured);
            self.0.lock().unwrap().push(captured);
        }
    }

    impl Visit for CapturedWarnEvent {
        fn record_str(&mut self, field: &Field, value: &str) {
            match field.name() {
                "event" => self.event = Some(value.to_string()),
                "message" => self.message = Some(value.to_string()),
                "generation" => self.generation = Some(value.to_string()),
                "scene_index" => self.scene_index = Some(value.to_string()),
                "scene_name" => self.scene_name = Some(value.to_string()),
                "observed_ping_count" => self.observed_ping_count = Some(value.to_string()),
                "timeout_ms" => self.timeout_ms = Some(value.to_string()),
                _ => {}
            }
        }

        fn record_i64(&mut self, field: &Field, value: i64) {
            self.record_str(field, &value.to_string());
        }

        fn record_u64(&mut self, field: &Field, value: u64) {
            self.record_str(field, &value.to_string());
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.record_str(field, format!("{value:?}").trim_matches('"'));
        }
    }

    async fn assert_pan_family_aux_event_does_not_override(parameter: FadeParameter) {
        let (event_bus, engine, mut rx) = spawn_runtime_for_test().await;
        let mut events = event_bus.subscribe();
        let (write_tx, mut write_rx) = tokio::sync::mpsc::channel::<Vec<Lv1ParameterWrite>>(8);

        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                match command {
                    Lv1Command::GetState { reply } => {
                        let _ = reply.send(Lv1StateSnapshot {
                            connection: ConnectionStatus::Connected,
                            scene: None,
                            scene_list: vec![],
                            channels: vec![],
                            ping_sequence: 0,
                        });
                    }
                    Lv1Command::WriteBatch(writes) => {
                        let _ = write_tx.send(writes).await;
                    }
                    _ => {}
                }
            }
        });

        start_fade(
            &engine,
            fade_config(
                scene(1, "Intro"),
                vec![
                    FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::Pan,
                        target: 45.0,
                    },
                    FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::Balance,
                        target: 45.0,
                    },
                    FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::Width,
                        target: 45.0,
                    },
                ],
                1000,
            ),
        )
        .await
        .unwrap();

        // Drain the initial fade setup writes and any immediate events before the aux report.
        while write_rx.try_recv().is_ok() {}
        while events.try_recv().is_ok() {}

        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: match parameter {
                FadeParameter::Balance => Lv1Event::BalanceChanged {
                    group: 0,
                    channel: 0,
                    balance: -45.0,
                },
                FadeParameter::Width => Lv1Event::WidthChanged {
                    group: 0,
                    channel: 0,
                    width: -45.0,
                },
                _ => unreachable!(),
            },
        });

        let writes = tokio::time::timeout(std::time::Duration::from_millis(1500), write_rx.recv())
            .await
            .expect("expected fade activity after auxiliary pan-family event")
            .expect("write signal should arrive");

        assert!(writes.iter().any(|write| {
            write.group == 0 && write.channel == 0 && write.parameter == Lv1WriteParameter::Pan
        }));
        assert!(writes.iter().any(|write| {
            write.group == 0 && write.channel == 0 && write.parameter == Lv1WriteParameter::Balance
        }));
        assert!(writes.iter().any(|write| {
            write.group == 0 && write.channel == 0 && write.parameter == Lv1WriteParameter::Width
        }));

        let mut saw_override = false;
        let mut saw_cancelled = false;
        while let Ok(event) = events.try_recv() {
            match event {
                AppEvent::Fade {
                    generation: 0,
                    event: FadeEvent::ChannelOverride { .. },
                } => saw_override = true,
                AppEvent::Fade {
                    generation: 0,
                    event: FadeEvent::ChannelCancelled { .. },
                } => saw_cancelled = true,
                _ => {}
            }
        }

        assert!(!saw_override, "unexpected ChannelOverride event");
        assert!(!saw_cancelled, "unexpected ChannelCancelled event");
    }

    #[tokio::test]
    async fn balance_report_does_not_cancel_pan_family_targets() {
        assert_pan_family_aux_event_does_not_override(FadeParameter::Balance).await;
    }

    #[tokio::test]
    async fn width_report_does_not_cancel_pan_family_targets() {
        assert_pan_family_aux_event_does_not_override(FadeParameter::Width).await;
    }

    #[tokio::test]
    async fn pan_report_cancels_balance_and_width_when_pan_target_is_missing() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let mut state = EngineState::new(event_bus, 7);
        let mut tick_interval = Some(tokio::time::interval(std::time::Duration::from_millis(40)));

        state
            .channels
            .push(active_pan_family_target(FadeParameter::Balance));
        state
            .channels
            .push(active_pan_family_target(FadeParameter::Width));
        state
            .channels
            .push(active_pan_family_target(FadeParameter::Pan));
        state.channels.last_mut().unwrap().group = 1;
        state.channels.last_mut().unwrap().channel = 1;

        let mut fade_completed_emitted = false;
        handle_pan_family_pan_report(
            &mut state,
            0,
            0,
            45.0,
            &mut tick_interval,
            &mut fade_completed_emitted,
        );

        assert_eq!(state.channels.len(), 1);
        assert!(
            state.channels.iter().any(|ch| ch.group == 1
                && ch.channel == 1
                && ch.key.parameter == FadeParameter::Pan)
        );

        let mut saw_override = false;
        let mut cancelled = std::collections::HashSet::new();
        while let Ok(event) = events.try_recv() {
            match event {
                AppEvent::Fade {
                    generation: 7,
                    event:
                        FadeEvent::ChannelOverride {
                            group,
                            channel,
                            parameter,
                        },
                } => {
                    assert_eq!((group, channel, parameter), (0, 0, FadeParameter::Pan));
                    saw_override = true;
                }
                AppEvent::Fade {
                    generation: 7,
                    event:
                        FadeEvent::ChannelCancelled {
                            group,
                            channel,
                            parameter,
                        },
                } => {
                    cancelled.insert((group, channel, parameter));
                }
                AppEvent::Fade {
                    generation: 7,
                    event: FadeEvent::FadeCompleted,
                } => {
                    panic!("unexpected FadeCompleted while unrelated target remains")
                }
                _ => {}
            }
        }

        assert!(saw_override, "missing ChannelOverride for pan");
        assert!(cancelled.contains(&(0, 0, FadeParameter::Balance)));
        assert!(cancelled.contains(&(0, 0, FadeParameter::Width)));
        assert!(!cancelled.contains(&(1, 1, FadeParameter::Pan)));
    }

    #[tokio::test]
    async fn pan_report_completes_when_no_active_targets_remain() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let mut state = EngineState::new(event_bus, 7);
        let mut tick_interval = Some(tokio::time::interval(std::time::Duration::from_millis(40)));

        state
            .channels
            .push(active_pan_family_target(FadeParameter::Balance));
        state
            .channels
            .push(active_pan_family_target(FadeParameter::Width));

        let mut fade_completed_emitted = false;
        handle_pan_family_pan_report(
            &mut state,
            0,
            0,
            45.0,
            &mut tick_interval,
            &mut fade_completed_emitted,
        );

        assert!(state.channels.is_empty());

        let mut saw_override = false;
        let mut cancelled = std::collections::HashSet::new();
        let mut saw_fade_completed = false;
        while let Ok(event) = events.try_recv() {
            match event {
                AppEvent::Fade {
                    generation: 7,
                    event:
                        FadeEvent::ChannelOverride {
                            group,
                            channel,
                            parameter,
                        },
                } => {
                    assert_eq!((group, channel, parameter), (0, 0, FadeParameter::Pan));
                    saw_override = true;
                }
                AppEvent::Fade {
                    generation,
                    event:
                        FadeEvent::ChannelCancelled {
                            group,
                            channel,
                            parameter,
                        },
                } => {
                    assert_eq!(generation, 7);
                    cancelled.insert((group, channel, parameter));
                }
                AppEvent::Fade {
                    generation,
                    event: FadeEvent::FadeCompleted,
                } => {
                    assert_eq!(generation, 7);
                    saw_fade_completed = true
                }
                _ => {}
            }
        }

        assert!(saw_override, "missing ChannelOverride for pan");
        assert!(cancelled.contains(&(0, 0, FadeParameter::Balance)));
        assert!(cancelled.contains(&(0, 0, FadeParameter::Width)));
        assert!(saw_fade_completed, "missing FadeCompleted");
    }

    #[tokio::test]
    async fn one_out_of_threshold_pan_report_does_not_cancel_active_pan_family_targets() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let mut state = EngineState::new(event_bus, 0);
        let mut tick_interval = Some(tokio::time::interval(std::time::Duration::from_millis(40)));

        state
            .channels
            .push(active_pan_family_target(FadeParameter::Pan));
        state
            .channels
            .push(active_pan_family_target(FadeParameter::Balance));
        state
            .channels
            .push(active_pan_family_target(FadeParameter::Width));

        let mut fade_completed_emitted = false;
        handle_pan_family_pan_report(
            &mut state,
            0,
            0,
            45.0,
            &mut tick_interval,
            &mut fade_completed_emitted,
        );

        assert_eq!(state.channels.len(), 3);
        assert!(
            state
                .channels
                .iter()
                .any(|ch| ch.key.parameter == FadeParameter::Pan)
        );
        assert!(
            state
                .channels
                .iter()
                .any(|ch| ch.key.parameter == FadeParameter::Balance)
        );
        assert!(
            state
                .channels
                .iter()
                .any(|ch| ch.key.parameter == FadeParameter::Width)
        );

        while let Ok(event) = events.try_recv() {
            match event {
                AppEvent::Fade {
                    generation: 0,
                    event: FadeEvent::ChannelOverride { .. },
                } => {
                    panic!("unexpected ChannelOverride event")
                }
                AppEvent::Fade {
                    generation: 0,
                    event: FadeEvent::ChannelCancelled { .. },
                } => {
                    panic!("unexpected ChannelCancelled event")
                }
                AppEvent::Fade {
                    generation: 0,
                    event: FadeEvent::FadeCompleted,
                } => {
                    panic!("unexpected FadeCompleted event")
                }
                _ => {}
            }
        }
    }

    #[tokio::test]
    async fn in_threshold_pan_report_resets_override_confirmation() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let mut state = EngineState::new(event_bus, 0);
        let mut tick_interval = Some(tokio::time::interval(std::time::Duration::from_millis(40)));

        state
            .channels
            .push(active_pan_family_target(FadeParameter::Pan));
        state
            .channels
            .push(active_pan_family_target(FadeParameter::Balance));
        state
            .channels
            .push(active_pan_family_target(FadeParameter::Width));

        let mut fade_completed_emitted = false;
        handle_pan_family_pan_report(
            &mut state,
            0,
            0,
            45.0,
            &mut tick_interval,
            &mut fade_completed_emitted,
        );
        handle_pan_family_pan_report(
            &mut state,
            0,
            0,
            0.0,
            &mut tick_interval,
            &mut fade_completed_emitted,
        );
        handle_pan_family_pan_report(
            &mut state,
            0,
            0,
            45.0,
            &mut tick_interval,
            &mut fade_completed_emitted,
        );

        assert_eq!(state.channels.len(), 3);
        let pan_target = state
            .channels
            .iter()
            .find(|ch| ch.key.parameter == FadeParameter::Pan)
            .expect("pan target should remain active");
        assert_eq!(pan_target.override_deviation_count, 1);

        while let Ok(event) = events.try_recv() {
            match event {
                AppEvent::Fade {
                    generation: 0,
                    event: FadeEvent::ChannelOverride { .. },
                } => {
                    panic!("unexpected ChannelOverride event")
                }
                AppEvent::Fade {
                    generation: 0,
                    event: FadeEvent::ChannelCancelled { .. },
                } => {
                    panic!("unexpected ChannelCancelled event")
                }
                AppEvent::Fade {
                    generation: 0,
                    event: FadeEvent::FadeCompleted,
                } => {
                    panic!("unexpected FadeCompleted event")
                }
                _ => {}
            }
        }
    }

    #[tokio::test]
    async fn pan_report_cancels_all_pan_family_targets_for_channel() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let mut state = EngineState::new(event_bus, 0);
        let mut tick_interval = Some(tokio::time::interval(std::time::Duration::from_millis(40)));
        state
            .channels
            .push(active_pan_family_target(FadeParameter::Pan));
        state
            .channels
            .push(active_pan_family_target(FadeParameter::Balance));
        state
            .channels
            .push(active_pan_family_target(FadeParameter::Width));
        state
            .channels
            .push(active_pan_family_target(FadeParameter::Pan));
        state.channels.last_mut().unwrap().group = 0;
        state.channels.last_mut().unwrap().channel = 1;

        let mut fade_completed_emitted = false;
        handle_pan_family_pan_report(
            &mut state,
            0,
            0,
            45.0,
            &mut tick_interval,
            &mut fade_completed_emitted,
        );

        assert_eq!(state.channels.len(), 4);

        handle_pan_family_pan_report(
            &mut state,
            0,
            0,
            45.0,
            &mut tick_interval,
            &mut fade_completed_emitted,
        );

        assert_eq!(state.channels.len(), 1);
        assert!(
            state.channels.iter().any(|ch| ch.group == 0
                && ch.channel == 1
                && ch.key.parameter == FadeParameter::Pan)
        );

        let mut saw_override = false;
        let mut cancelled = std::collections::HashSet::new();
        let mut saw_fade_completed = false;
        while let Ok(event) = events.try_recv() {
            match event {
                AppEvent::Fade {
                    generation: 0,
                    event:
                        FadeEvent::ChannelOverride {
                            group,
                            channel,
                            parameter,
                        },
                } => {
                    assert_eq!((group, channel, parameter), (0, 0, FadeParameter::Pan));
                    saw_override = true;
                }
                AppEvent::Fade {
                    generation: 0,
                    event:
                        FadeEvent::ChannelCancelled {
                            group,
                            channel,
                            parameter,
                        },
                } => {
                    cancelled.insert((group, channel, parameter));
                }
                AppEvent::Fade {
                    generation: 0,
                    event: FadeEvent::FadeCompleted,
                } => saw_fade_completed = true,
                _ => {}
            }
        }

        assert!(saw_override, "missing ChannelOverride for pan");
        assert!(cancelled.contains(&(0, 0, FadeParameter::Pan)));
        assert!(cancelled.contains(&(0, 0, FadeParameter::Balance)));
        assert!(cancelled.contains(&(0, 0, FadeParameter::Width)));
        assert!(!cancelled.contains(&(0, 1, FadeParameter::Pan)));
        assert!(!saw_fade_completed, "unexpected FadeCompleted");
    }

    #[tokio::test]
    async fn timed_fade_sends_due_writes_in_one_batch() {
        let (event_bus, engine, mut rx) = spawn_runtime_for_test().await;
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let mut result_tx = Some(result_tx);
            while let Some(command) = rx.recv().await {
                match command {
                    Lv1Command::GetState { reply } => {
                        let _ = reply.send(Lv1StateSnapshot {
                            connection: ConnectionStatus::Connected,
                            scene: None,
                            scene_list: vec![],
                            channels: vec![],
                            ping_sequence: 0,
                        });
                    }
                    Lv1Command::WriteBatch(writes) => {
                        let _ = result_tx.take().unwrap().send(writes);
                        break;
                    }
                    _ => panic!("expected GetState followed by WriteBatch"),
                }
            }
        });

        start_fade(
            &engine,
            fade_config(
                scene(1, "Intro"),
                vec![
                    FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::FaderDb,
                        target: -12.5,
                    },
                    FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::Pan,
                        target: 15.0,
                    },
                ],
                120,
            ),
        )
        .await
        .unwrap();

        let writes = tokio::time::timeout(std::time::Duration::from_secs(2), result_rx)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            writes,
            vec![
                Lv1ParameterWrite {
                    group: 0,
                    channel: 0,
                    parameter: Lv1WriteParameter::FaderDb,
                    value: -12.5,
                },
                Lv1ParameterWrite {
                    group: 0,
                    channel: 0,
                    parameter: Lv1WriteParameter::Pan,
                    value: 15.0,
                },
            ]
        );

        let _ = event_bus;
    }

    #[tokio::test]
    async fn zero_duration_fade_sends_all_parameters_in_one_batch() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let event_bus = AppEventBus::default();
        let lv1 = test_actor_handle(tx);
        let runtime_generation = RuntimeGeneration::new();
        let (engine, task, peers) = build_engine(runtime_generation, event_bus.clone(), 0);
        peers.set_lv1(lv1);
        task.spawn();

        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let mut result_tx = Some(result_tx);
            while let Some(command) = rx.recv().await {
                match command {
                    Lv1Command::GetState { reply } => {
                        let _ = reply.send(Lv1StateSnapshot {
                            connection: ConnectionStatus::Connected,
                            scene: None,
                            scene_list: vec![],
                            channels: vec![],
                            ping_sequence: 0,
                        });
                    }
                    Lv1Command::WriteBatch(writes) => {
                        let _ = result_tx.take().unwrap().send(writes);
                        break;
                    }
                    _ => panic!("expected GetState followed by WriteBatch"),
                }
            }
        });

        start_fade(
            &engine,
            fade_config(
                scene(1, "Intro"),
                vec![
                    FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::FaderDb,
                        target: -12.5,
                    },
                    FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::Pan,
                        target: 15.0,
                    },
                    FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::Balance,
                        target: -10.0,
                    },
                    FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::Width,
                        target: 0.75,
                    },
                ],
                0,
            ),
        )
        .await
        .unwrap();

        let writes = tokio::time::timeout(std::time::Duration::from_secs(1), result_rx)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            writes,
            vec![
                Lv1ParameterWrite {
                    group: 0,
                    channel: 0,
                    parameter: Lv1WriteParameter::FaderDb,
                    value: -12.5,
                },
                Lv1ParameterWrite {
                    group: 0,
                    channel: 0,
                    parameter: Lv1WriteParameter::Pan,
                    value: 15.0,
                },
                Lv1ParameterWrite {
                    group: 0,
                    channel: 0,
                    parameter: Lv1WriteParameter::Balance,
                    value: -10.0,
                },
                Lv1ParameterWrite {
                    group: 0,
                    channel: 0,
                    parameter: Lv1WriteParameter::Width,
                    value: 0.75,
                },
            ]
        );
    }

    #[tokio::test]
    async fn stale_expected_generation_is_rejected_before_lv1_state_lookup() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        let lv1 = test_actor_handle(lv1_tx);
        runtime_generation.set(3).await;

        let mut state = EngineState::new(event_bus, 0);
        let result = handle_recall_scene_fade(
            &runtime_generation,
            &lv1,
            &mut state,
            fade_config(
                scene(1, "Intro"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -12.5,
                }],
                120,
            ),
            Some(2),
        )
        .await;

        assert_eq!(result, Err(AppCommandError::StaleGeneration));
        assert!(lv1_rx.try_recv().is_err());
        assert!(state.channels.is_empty());
    }

    #[tokio::test]
    async fn generation_flip_while_lv1_snapshot_is_pending_is_rejected_after_snapshot() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        let lv1 = test_actor_handle(lv1_tx);
        runtime_generation.set(3).await;

        let runtime_generation_for_lv1 = runtime_generation.clone();
        tokio::spawn(async move {
            if let Some(Lv1Command::GetState { reply }) = lv1_rx.recv().await {
                runtime_generation_for_lv1.set(4).await;
                let _ = reply.send(Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: vec![],
                    channels: vec![],
                    ping_sequence: 0,
                });
            }
        });

        let mut state = EngineState::new(event_bus, 0);
        let result = handle_recall_scene_fade(
            &runtime_generation,
            &lv1,
            &mut state,
            fade_config(
                scene(1, "Intro"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -12.5,
                }],
                120,
            ),
            Some(2),
        )
        .await;

        assert_eq!(result, Err(AppCommandError::StaleGeneration));
        assert!(state.channels.is_empty());
    }

    #[tokio::test]
    async fn zero_duration_recall_fade_uses_generation_checked_write_batch() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        let lv1 = test_actor_handle(lv1_tx);
        runtime_generation.set(4).await;

        let (write_tx, write_rx) = tokio::sync::oneshot::channel::<()>();
        let write_tx = std::sync::Arc::new(std::sync::Mutex::new(Some(write_tx)));
        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                if let Lv1Command::WriteBatch(_) = command
                    && let Some(tx) = write_tx.lock().unwrap().take()
                {
                    let _ = tx.send(());
                    break;
                }
            }
        });

        let mut state = EngineState::new(event_bus, 0);
        state.channels.push(ActiveTarget::new(ActiveTargetInit {
            key: FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -12.5,
            }
            .key(),
            group: 0,
            channel: 0,
            start_value: -20.0,
            target_value: -12.5,
            curve: FadeCurve::Linear,
            duration: std::time::Duration::from_millis(0),
            started_at: Instant::now(),
            expected_generation: Some(3),
        }));

        let sent = send_batch_if_generation(
            &runtime_generation,
            &lv1,
            &state.event_bus,
            3,
            vec![build_parameter_write(0, 0, FadeParameter::FaderDb, -12.5)],
        )
        .await;
        assert!(!sent);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), write_rx)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn complete_fade_is_idempotent() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let mut state = EngineState::new(event_bus.clone(), 0);
        let mut tick_interval = None;
        let mut emitted = false;

        complete_fade(&mut tick_interval, &mut state, &mut emitted);
        complete_fade(&mut tick_interval, &mut state, &mut emitted);
        let event = tokio::time::timeout(std::time::Duration::from_millis(100), events.recv())
            .await
            .unwrap()
            .unwrap();

        assert!(matches!(
            event,
            AppEvent::Fade {
                generation: 0,
                event: FadeEvent::FadeCompleted
            }
        ));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), events.recv())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn timed_recall_fade_tick_uses_generation_checked_write_batch() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        let lv1 = test_actor_handle(lv1_tx);
        runtime_generation.set(3).await;

        let (write_tx, write_rx) = tokio::sync::oneshot::channel::<()>();
        let write_tx = std::sync::Arc::new(std::sync::Mutex::new(Some(write_tx)));
        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                if let Lv1Command::WriteBatch(_) = command
                    && let Some(tx) = write_tx.lock().unwrap().take()
                {
                    let _ = tx.send(());
                    break;
                }
            }
        });

        let mut state = EngineState::new(event_bus, 0);
        state.channels.push(ActiveTarget::new(ActiveTargetInit {
            key: FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -12.5,
            }
            .key(),
            group: 0,
            channel: 0,
            start_value: -20.0,
            target_value: -12.5,
            curve: FadeCurve::Linear,
            duration: std::time::Duration::from_millis(120),
            started_at: Instant::now(),
            expected_generation: Some(3),
        }));

        runtime_generation.set(4).await;
        let writes = vec![build_parameter_write(0, 0, FadeParameter::FaderDb, -12.5)];
        let sent =
            send_batch_if_generation(&runtime_generation, &lv1, &state.event_bus, 3, writes).await;
        assert!(!sent);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), write_rx)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn mixed_generation_writes_on_same_tick_route_separately() {
        let _event_bus = AppEventBus::default();

        let mut state = EngineState::new(AppEventBus::default(), 0);
        state.channels.push(ActiveTarget::new(ActiveTargetInit {
            key: FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -12.5,
            }
            .key(),
            group: 0,
            channel: 0,
            start_value: -20.0,
            target_value: -12.5,
            curve: FadeCurve::Linear,
            duration: std::time::Duration::from_millis(120),
            started_at: Instant::now(),
            expected_generation: Some(3),
        }));
        state.channels.push(ActiveTarget::new(ActiveTargetInit {
            key: FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: -10.0,
            }
            .key(),
            group: 0,
            channel: 1,
            start_value: -15.0,
            target_value: -10.0,
            curve: FadeCurve::Linear,
            duration: std::time::Duration::from_millis(120),
            started_at: Instant::now(),
            expected_generation: None,
        }));

        let writes = vec![
            build_parameter_write(0, 0, FadeParameter::FaderDb, -12.5),
            build_parameter_write(0, 1, FadeParameter::FaderDb, -10.0),
        ];

        let grouped = group_writes_by_generation(&state.channels, writes);
        assert_eq!(grouped.len(), 2);
        assert!(grouped.iter().any(|(generation, _)| *generation == Some(3)));
        assert!(grouped.iter().any(|(generation, _)| generation.is_none()));
    }

    #[tokio::test]
    async fn stale_checked_write_cancels_generation_owned_targets() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        let lv1 = test_actor_handle(lv1_tx);

        let mut state = EngineState::new(event_bus, 0);
        state.channels.push(ActiveTarget::new(ActiveTargetInit {
            key: FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -12.5,
            }
            .key(),
            group: 0,
            channel: 0,
            start_value: -20.0,
            target_value: -12.5,
            curve: FadeCurve::Linear,
            duration: std::time::Duration::from_millis(120),
            started_at: Instant::now(),
            expected_generation: Some(3),
        }));

        runtime_generation.set(4).await;
        let writes = vec![build_parameter_write(0, 0, FadeParameter::FaderDb, -12.5)];
        let sent =
            send_batch_if_generation(&runtime_generation, &lv1, &state.event_bus, 3, writes).await;
        assert!(!sent);
        cancel_generation_owned_targets(&mut state, 3);

        assert!(state.channels.is_empty());
        assert!(lv1_rx.try_recv().is_err());
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn post_recall_ping_timeout_aborts_fade_and_logs_warning() {
        let captured = CapturedWarnEvents::default();
        let subscriber = Registry::default().with(captured.clone());
        let _guard = tracing::subscriber::set_default(subscriber);
        let (event_bus, engine, mut write_rx) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(
                40,
                vec![channel_info(0, -20.0, None)],
            )])
            .await;
        let mut events = event_bus.subscribe();

        start_fade_for_generation(
            &engine,
            fade_config(
                scene(1, "Intro"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -10.0,
                }],
                1_000,
            ),
            Some(7),
        )
        .await
        .unwrap();

        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(5)).await;
        wait_for_fade_aborted(&mut events).await;
        assert_no_write_after_cancellation(&mut write_rx).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 41 },
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 42 },
        });
        assert_no_write_after_cancellation(&mut write_rx).await;
        assert_no_additional_fade_abort(&mut events).await;

        let warnings = captured.0.lock().unwrap();
        let timeout_warnings: Vec<_> = warnings
            .iter()
            .filter(|warning| {
                warning.level.as_deref() == Some("WARN")
                    && warning.event.as_deref() == Some("fade_post_recall_ping_timeout")
            })
            .collect();
        assert_eq!(
            timeout_warnings,
            vec![&CapturedWarnEvent {
                level: Some("WARN".to_string()),
                event: Some("fade_post_recall_ping_timeout".to_string()),
                message: Some(
                    "Fades were aborted because LV1 did not resume its keepalive cadence after scene recall"
                        .to_string(),
                ),
                generation: Some("7".to_string()),
                scene_index: Some("1".to_string()),
                scene_name: Some("Intro".to_string()),
                observed_ping_count: Some("0".to_string()),
                timeout_ms: Some("5000".to_string()),
            }]
        );
    }

    #[tokio::test]
    async fn post_recall_ping_abort_clears_readiness_before_later_pings() {
        let (event_bus, engine, mut write_rx) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(
                40,
                vec![channel_info(0, -20.0, None)],
            )])
            .await;
        let mut events = event_bus.subscribe();

        start_fade_for_generation(
            &engine,
            fade_config(
                scene(1, "Intro"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -10.0,
                }],
                1_000,
            ),
            Some(7),
        )
        .await
        .unwrap();

        let (reply, reply_rx) = tokio::sync::oneshot::channel();
        engine
            .send(FadeCommand::AbortAll { reply: Some(reply) })
            .await
            .unwrap();
        reply_rx.await.unwrap().unwrap();
        wait_for_fade_aborted(&mut events).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 41 },
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 42 },
        });
        assert_no_write_after_cancellation(&mut write_rx).await;
        assert_no_additional_fade_abort(&mut events).await;
    }

    #[tokio::test]
    async fn post_recall_ping_stale_disconnect_keeps_current_gate_and_releases_after_pings() {
        let (event_bus, engine, mut write_rx) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(
                40,
                vec![channel_info(0, -20.0, None)],
            )])
            .await;
        let mut events = event_bus.subscribe();

        start_fade_for_generation(
            &engine,
            fade_config(
                scene(1, "Intro"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -10.0,
                }],
                1_000,
            ),
            Some(7),
        )
        .await
        .unwrap();

        event_bus.publish(AppEvent::Lv1 {
            generation: 6,
            event: Lv1Event::Disconnected {
                reason: "stale test disconnect".to_string(),
            },
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 41 },
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 42 },
        });

        let writes = tokio::time::timeout(Duration::from_millis(300), write_rx.recv())
            .await
            .expect("current fade should resume after valid pings")
            .expect("fade write channel should remain open");
        assert!(writes.iter().any(|write| {
            write.group == 0 && write.channel == 0 && write.parameter == Lv1WriteParameter::FaderDb
        }));
        assert_no_additional_fade_abort(&mut events).await;
    }

    #[tokio::test]
    async fn post_recall_ping_matching_disconnect_aborts_and_prevents_writes() {
        let (event_bus, engine, mut write_rx) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(
                40,
                vec![channel_info(0, -20.0, None)],
            )])
            .await;
        let mut events = event_bus.subscribe();

        start_fade_for_generation(
            &engine,
            fade_config(
                scene(1, "Intro"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -10.0,
                }],
                1_000,
            ),
            Some(7),
        )
        .await
        .unwrap();

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::Disconnected {
                reason: "test disconnect".to_string(),
            },
        });
        wait_for_fade_aborted(&mut events).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 41 },
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 42 },
        });
        assert_no_write_after_cancellation(&mut write_rx).await;
        assert_no_additional_fade_abort(&mut events).await;
    }

    #[tokio::test]
    async fn post_recall_ping_generation_change_clears_readiness_before_later_pings() {
        let (event_bus, engine, mut write_rx) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(
                40,
                vec![channel_info(0, -20.0, None)],
            )])
            .await;
        let mut events = event_bus.subscribe();

        start_fade_for_generation(
            &engine,
            fade_config(
                scene(1, "Intro"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -10.0,
                }],
                1_000,
            ),
            Some(7),
        )
        .await
        .unwrap();

        event_bus.publish(AppEvent::Runtime(
            RuntimeLifecycleEvent::ActiveGenerationChanged { generation: 8 },
        ));
        wait_for_fade_aborted(&mut events).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 41 },
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 42 },
        });
        assert_no_write_after_cancellation(&mut write_rx).await;
    }

    #[tokio::test]
    async fn post_recall_ping_gate_requires_two_newer_same_generation_pings() {
        let (event_bus, engine, mut write_rx) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(
                40,
                vec![channel_info(0, -20.0, None)],
            )])
            .await;

        start_fade_for_generation(
            &engine,
            fade_config(
                scene(1, "Intro"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -10.0,
                }],
                1_000,
            ),
            Some(7),
        )
        .await
        .unwrap();

        assert_no_write(&mut write_rx).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 40 },
        });
        assert_no_write(&mut write_rx).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 41 },
        });
        assert_no_write(&mut write_rx).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 6,
            event: Lv1Event::PingReceived { sequence: 42 },
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 41 },
        });
        assert_no_write(&mut write_rx).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 42 },
        });
        let writes = tokio::time::timeout(Duration::from_millis(300), write_rx.recv())
            .await
            .expect("fade should resume after two newer pings")
            .expect("fade write channel should remain open");
        assert!(writes.iter().any(|write| {
            write.group == 0 && write.channel == 0 && write.parameter == Lv1WriteParameter::FaderDb
        }));
    }

    #[tokio::test]
    async fn post_recall_ping_gate_preserves_target_scoped_manual_overrides() {
        let (event_bus, engine, mut write_rx) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(
                40,
                vec![
                    channel_info(0, -20.0, Some(0.0)),
                    channel_info(1, -20.0, Some(0.0)),
                ],
            )])
            .await;
        let mut events = event_bus.subscribe();

        start_fade_for_generation(
            &engine,
            fade_config(
                scene(1, "Intro"),
                vec![
                    FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::FaderDb,
                        target: -10.0,
                    },
                    FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::Pan,
                        target: 45.0,
                    },
                    FadeTarget {
                        group: 0,
                        channel: 1,
                        parameter: FadeParameter::FaderDb,
                        target: -10.0,
                    },
                    FadeTarget {
                        group: 0,
                        channel: 1,
                        parameter: FadeParameter::Pan,
                        target: 45.0,
                    },
                ],
                5_000,
            ),
            Some(7),
        )
        .await
        .unwrap();

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::FaderChanged {
                group: 0,
                channel: 0,
                gain_db: -50.0,
            },
        });
        for _ in 0..2 {
            event_bus.publish(AppEvent::Lv1 {
                generation: 7,
                event: Lv1Event::PanChanged {
                    group: 0,
                    channel: 1,
                    pan: -90.0,
                },
            });
        }

        let mut overridden = std::collections::HashSet::new();
        let mut cancelled = std::collections::HashSet::new();
        tokio::time::timeout(Duration::from_millis(300), async {
            while overridden.len() < 2 || cancelled.len() < 2 {
                match events.recv().await.expect("event bus should remain open") {
                    AppEvent::Fade {
                        event:
                            FadeEvent::ChannelOverride {
                                channel, parameter, ..
                            },
                        ..
                    } => {
                        overridden.insert((channel, parameter));
                    }
                    AppEvent::Fade {
                        event:
                            FadeEvent::ChannelCancelled {
                                channel, parameter, ..
                            },
                        ..
                    } => {
                        cancelled.insert((channel, parameter));
                    }
                    _ => {}
                }
            }
        })
        .await
        .expect("manual override behavior should remain active while readiness waits");
        assert!(overridden.contains(&(0, FadeParameter::FaderDb)));
        assert!(overridden.contains(&(1, FadeParameter::Pan)));
        assert!(cancelled.contains(&(0, FadeParameter::FaderDb)));
        assert!(cancelled.contains(&(1, FadeParameter::Pan)));

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 41 },
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 42 },
        });

        let writes = tokio::time::timeout(Duration::from_millis(300), write_rx.recv())
            .await
            .expect("fade should resume after readiness release")
            .expect("fade write channel should remain open");
        assert!(writes.iter().any(|write| {
            write.parameter == Lv1WriteParameter::Pan && write.group == 0 && write.channel == 0
        }));
        assert!(writes.iter().any(|write| {
            write.parameter == Lv1WriteParameter::FaderDb && write.group == 0 && write.channel == 1
        }));
        assert!(!writes.iter().any(|write| {
            write.parameter == Lv1WriteParameter::FaderDb && write.group == 0 && write.channel == 0
        }));
        assert!(!writes.iter().any(|write| {
            write.parameter == Lv1WriteParameter::Pan && write.group == 0 && write.channel == 1
        }));
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn post_recall_ping_gate_newer_recall_receives_a_full_timeout_window() {
        let (event_bus, engine, _write_rx) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(40, vec![channel_info(0, -20.0, None)]),
            connected_snapshot(41, vec![channel_info(0, -20.0, None)]),
        ])
        .await;
        let mut events = event_bus.subscribe();

        start_fade_for_generation(
            &engine,
            fade_config(
                scene(1, "Scene A"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -10.0,
                }],
                1_000,
            ),
            Some(7),
        )
        .await
        .unwrap();

        tokio::time::advance(Duration::from_secs(4)).await;

        start_fade_for_generation(
            &engine,
            fade_config(
                scene(2, "Scene B"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -5.0,
                }],
                1_000,
            ),
            Some(7),
        )
        .await
        .unwrap();

        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(4_999)).await;
        tokio::task::yield_now().await;
        assert_no_additional_fade_abort(&mut events).await;

        tokio::time::advance(Duration::from_millis(1)).await;
        wait_for_fade_aborted(&mut events).await;
    }

    #[tokio::test]
    async fn post_recall_ping_gate_resets_after_another_timed_recall() {
        let (event_bus, engine, mut write_rx) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(40, vec![channel_info(0, -20.0, None)]),
            connected_snapshot(41, vec![channel_info(0, -20.0, None)]),
        ])
        .await;

        start_fade_for_generation(
            &engine,
            fade_config(
                scene(1, "Scene A"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -10.0,
                }],
                1_000,
            ),
            Some(7),
        )
        .await
        .unwrap();
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 41 },
        });

        start_fade_for_generation(
            &engine,
            fade_config(
                scene(2, "Scene B"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -5.0,
                }],
                1_000,
            ),
            Some(7),
        )
        .await
        .unwrap();
        assert_no_write(&mut write_rx).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 42 },
        });
        assert_no_write(&mut write_rx).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 43 },
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(300), write_rx.recv())
                .await
                .is_ok(),
            "two newer pings after the latest recall should release the gate"
        );
    }

    #[tokio::test]
    async fn post_recall_ping_reset_rebases_unrelated_fade_and_replaces_overlap() {
        let snapshots = vec![
            connected_snapshot(
                40,
                vec![
                    channel_info(0, 0.0, Some(0.0)),
                    channel_info(1, 0.0, Some(0.0)),
                ],
            ),
            connected_snapshot(
                42,
                vec![
                    channel_info(0, 0.0, Some(0.0)),
                    channel_info(1, 0.0, Some(0.0)),
                ],
            ),
        ];
        let (event_bus, engine, mut write_rx) = spawn_runtime_for_ping_gate_test(snapshots).await;

        start_fade_for_generation(
            &engine,
            fade_config(
                scene(1, "Scene A"),
                vec![
                    FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::Pan,
                        target: 40.0,
                    },
                    FadeTarget {
                        group: 0,
                        channel: 1,
                        parameter: FadeParameter::Pan,
                        target: 40.0,
                    },
                ],
                1_000,
            ),
            Some(7),
        )
        .await
        .unwrap();
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 41 },
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 42 },
        });

        let first_a_writes = tokio::time::timeout(Duration::from_millis(300), write_rx.recv())
            .await
            .expect("Scene A should write after its readiness gate releases")
            .expect("fade write channel should remain open");
        let a_before_gate = first_a_writes
            .iter()
            .find(|write| write.channel == 1 && write.parameter == Lv1WriteParameter::Pan)
            .expect("Scene A's unrelated target should write")
            .value;

        start_fade_for_generation(
            &engine,
            fade_config(
                scene(2, "Scene B"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::Pan,
                    target: -40.0,
                }],
                1_000,
            ),
            Some(7),
        )
        .await
        .unwrap();
        while write_rx.try_recv().is_ok() {}
        assert_no_write(&mut write_rx).await;

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_no_write(&mut write_rx).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 43 },
        });
        assert_no_write(&mut write_rx).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 44 },
        });
        let post_release_writes = tokio::time::timeout(Duration::from_millis(300), async {
            loop {
                let writes = write_rx
                    .recv()
                    .await
                    .expect("fade write channel should remain open");
                if writes.iter().any(|write| {
                    write.channel == 0
                        && write.parameter == Lv1WriteParameter::Pan
                        && write.value < 0.0
                }) && writes
                    .iter()
                    .any(|write| write.channel == 1 && write.parameter == Lv1WriteParameter::Pan)
                {
                    return writes;
                }
            }
        })
        .await
        .expect("Scene B should write after its replacement gate releases");
        let resumed_a_value = post_release_writes
            .iter()
            .find(|write| write.channel == 1 && write.parameter == Lv1WriteParameter::Pan)
            .expect("Scene A's unrelated target should resume")
            .value;
        let replacement_b_value = post_release_writes
            .iter()
            .find(|write| write.channel == 0 && write.parameter == Lv1WriteParameter::Pan)
            .expect("Scene B's overlapping target should replace Scene A")
            .value;

        assert!(replacement_b_value < 0.0, "Scene B should own channel 0");
        assert!(
            resumed_a_value > a_before_gate,
            "Scene A's unrelated target should make progress after release"
        );
        assert!(
            resumed_a_value <= a_before_gate + 3.0,
            "Scene A must resume from pre-pause progress, not wall-clock progress"
        );
    }
}

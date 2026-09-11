//! Fade engine actor — animates LV1 faders over time.

use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::fade::FadeEngineHandle;
use crate::fade::commands::{
    FadeCommand, RecallReadinessCancellation, RecallReadinessRequest, SameSceneRecallBehavior,
};
use crate::fade::events::FadeEvent;
use crate::fade::state::{EngineState, PingGateProgress, READINESS_PINGS_REQUIRED};
use crate::fade::tick::{ActiveTarget, ActiveTargetInit, TICK_HZ};
use crate::fade::types::{FadeParameter, FadeTarget};
use crate::lv1::{
    Lv1ActorHandle, Lv1Command, Lv1Connection, Lv1Event, Lv1ParameterWrite, Lv1WriteParameter,
};
use crate::runtime::errors::AppCommandError;
use crate::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use crate::runtime::generation::RuntimeGeneration;

pub struct FadeEngineTask {
    connection: Lv1Connection,
    event_bus: AppEventBus,
    cmd_rx: mpsc::Receiver<FadeCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecallSceneFadeOutcome {
    Started,
    Finishing { target_count: usize },
    Overriding { target_count: usize },
}

impl FadeEngineTask {
    pub fn spawn(self) {
        tokio::spawn(run_engine(self.connection, self.event_bus, self.cmd_rx));
    }
}

pub fn build_engine(
    runtime_generation: RuntimeGeneration,
    event_bus: AppEventBus,
    generation: u64,
    lv1: Lv1ActorHandle,
) -> (FadeEngineHandle, FadeEngineTask) {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    let handle = cmd_tx;
    let task = FadeEngineTask {
        connection: Lv1Connection::new(lv1, runtime_generation, generation),
        event_bus,
        cmd_rx,
    };
    (handle, task)
}

/**
 * @cc [owner:mixxorz,label:safety;reliability] generation-fenced-effects
 * The engine MUST ignore LV1 feedback from other generations and MUST cancel active or paused work
 * on a matching disconnect, generation revocation, or actor shutdown without making later writes.
 * Successful start, channel-completion, fade-completion, and write-failure publication MUST occur
 * only while this engine's generation remains current.
 */
/**
 * @cc [owner:mixxorz,label:safety;product] fader-manual-override-lifecycle
 * A matching-generation fader report beyond the position-space override threshold MUST remove only
 * that group/channel's fader target, publish `ChannelOverride` followed by `ChannelCancelled`, and
 * publish terminal fade completion if no active targets remain.
 */
/**
 * @cc [owner:mixxorz,label:safety] override-feedback-during-readiness
 * Matching-generation fader and pan feedback MUST continue to apply manual-override cancellation
 * while readiness pauses interpolation. Removing targets MUST NOT remove the readiness barrier, and
 * any targets that remain MUST stay paused until readiness releases.
 */
/**
 * @cc [owner:mixxorz,label:safety;reliability] tick-write-failure-cancels
 * If a checked tick write or current-generation check fails, the tick MUST cancel every active
 * target, publish `ChannelCancelled` for each removed target and `FadeAborted`, and MUST NOT publish
 * `ChannelCompleted` or `FadeCompleted` for that tick.
 */
/**
 * @cc [owner:mixxorz,label:product;safety] successful-tick-terminal-order
 * Each tick MUST place all due parameter values in one checked write batch. Only after that batch or
 * an empty-batch generation check succeeds MAY it remove exact-finished targets and publish their
 * `ChannelCompleted` facts, followed by at most one `FadeCompleted` when no targets remain.
 */
/**
 * @cc [owner:mixxorz,label:product] targetless-zero-duration-events
 * After an admitted targetless or zero-duration recall, the engine MUST publish `FadeStarted` only
 * when active targets remain. With no active targets it MUST defer terminal completion while a
 * readiness barrier exists; otherwise it MAY close the current idle epoch with at most one
 * `FadeCompleted`.
 */
async fn run_engine(
    connection: Lv1Connection,
    event_bus: AppEventBus,
    mut cmd_rx: mpsc::Receiver<FadeCommand>,
) {
    let generation = connection.generation();
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
            biased;
            cmd = cmd_rx.recv() => {
                match cmd {
                    None => break,
                    Some(FadeCommand::RecallSceneFade { config, same_scene_behavior, readiness, reply }) => {
                        let scene_index = config.scene.index;
                        let scene_name = config.scene.name.clone();
                        let duration_ms = config.duration_ms;
                        let target_count = config.targets.len();
                        let result = handle_recall_scene_fade(&connection, &mut state, config, same_scene_behavior, readiness).await;

                        let result = match result {
                            Ok(outcome) => connection.if_current(|| {
                                if state.is_active() {
                                    let mut interval = tokio::time::interval(Duration::from_millis(1000 / TICK_HZ));
                                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                                    tick_interval = Some(interval);
                                    fade_completed_emitted = false;
                                    match outcome {
                                        RecallSceneFadeOutcome::Started => tracing::info!(event = "fade_started", scene_index = scene_index, scene_name = %scene_name, duration_ms = duration_ms, target_count = target_count, "Fade started for {}: {} ({} targets, {} ms)", scene_index, scene_name, target_count, duration_ms),
                                        RecallSceneFadeOutcome::Finishing { target_count } => tracing::info!(event = "fade_same_scene_finishing", scene_index = scene_index, scene_name = %scene_name, target_count, "Repeated scene recall is finishing active fade targets for {}: {} ({} targets)", scene_index, scene_name, target_count),
                                        RecallSceneFadeOutcome::Overriding { target_count } => tracing::info!(event = "fade_same_scene_overriding", scene_index = scene_index, scene_name = %scene_name, target_count, "Repeated scene recall is overriding active fade targets from their current values for {}: {} ({} targets)", scene_index, scene_name, target_count),
                                    }
                                    state.fan_out(FadeEvent::FadeStarted);
                                } else {
                                    tick_interval = None;
                                    if !state.is_waiting_for_readiness() {
                                        complete_fade(&mut tick_interval, &mut state, &mut fade_completed_emitted);
                                    }
                                }
                            }).await.ok_or(AppCommandError::StaleGeneration),
                            Err(err) => Err(err),
                        };
                        if let Some(reply) = reply {
                            let _ = reply.send(result);
                        }
                    }
                    Some(FadeCommand::WaitForRecallReadiness { scene, readiness, reply }) => {
                        let result = handle_wait_for_recall_readiness(
                            &connection,
                            &mut state,
                            scene,
                            readiness,
                        )
                        .await;
                        if let Some(reply) = reply {
                            let _ = reply.send(result);
                        }
                    }
                    Some(FadeCommand::AbortAll { reply }) => {
                        state.cancel_all_in_place(RecallReadinessCancellation::Aborted);
                        tick_interval = None;
                        state.fan_out(FadeEvent::FadeAborted);
                        if let Some(reply) = reply {
                            let _ = reply.send(Ok(()));
                        }
                    }
                }
            }

            app_event = app_events.recv() => {
                match app_event {
                    Ok(AppEvent::Lv1 {
                        generation: event_generation,
                        event: Lv1Event::FaderChanged { group, channel, gain_db },
                    }) if event_generation == generation => {
                        if let Some(pos) = state.channels.iter().position(|ch| ch.key.group == group && ch.key.channel == channel && ch.key.parameter == FadeParameter::FaderDb)
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
                                fade_completed_emitted = false;
                                complete_fade(&mut tick_interval, &mut state, &mut fade_completed_emitted);
                            }
                        }
                    }
                    Ok(AppEvent::Lv1 {
                        generation: event_generation,
                        event: Lv1Event::PanChanged { group, channel, pan },
                    }) if event_generation == generation => {
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
                            state.cancel_all_in_place(RecallReadinessCancellation::Disconnected);
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
                            state.cancel_all_in_place(RecallReadinessCancellation::GenerationChanged);
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
                        state.mark_readiness_lagged();
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }

            _ = readiness_timeout_fut => {
                if let Some(context) = state.timeout_readiness() {
                    tick_interval = None;
                    fade_completed_emitted = false;
                    if context.completion_owned {
                        tracing::debug!(
                            event = "fade_post_recall_ping_timeout",
                            generation = context.generation,
                            scene_index = context.scene_index,
                            scene_name = %context.scene_name,
                            observed_ping_count = context.observed_ping_count,
                            timeout_ms = context.timeout_ms,
                            "Fade readiness timed out after scene recall"
                        );
                    } else {
                        tracing::warn!(
                            event = "fade_post_recall_ping_timeout",
                            generation = context.generation,
                            scene_index = context.scene_index,
                            scene_name = %context.scene_name,
                            observed_ping_count = context.observed_ping_count,
                            timeout_ms = context.timeout_ms,
                            "Fades were aborted because LV1 did not resume its keepalive cadence after scene recall"
                        );
                    }
                    state.fan_out(FadeEvent::FadeAborted);
                }
            }

            _ = tick_fut => {
                if state.is_waiting_for_readiness() {
                    continue;
                }

                let now = Instant::now();
                let mut completed_targets = Vec::new();
                let mut writes = Vec::new();

                for ch in &mut state.channels {
                    if ch.is_done(now) {
                        let target_db = ch.exact_final_send();
                        writes.push(build_parameter_write(ch.key.group, ch.key.channel, ch.key.parameter, target_db));
                        completed_targets.push(ch.key.clone());
                        continue;
                    }

                    if let Some(new_value) = ch.next_send(now) {
                        writes.push(build_parameter_write(ch.key.group, ch.key.channel, ch.key.parameter, new_value));
                    }
                }

                let sent = if writes.is_empty() {
                    connection.ensure_current().await
                } else {
                    send_batch(&connection, &state.event_bus, writes).await
                };
                if let Err(error) = sent {
                    let reason = if error == AppCommandError::StaleGeneration {
                        RecallReadinessCancellation::GenerationChanged
                    } else {
                        RecallReadinessCancellation::Disconnected
                    };
                    for target in std::mem::take(&mut state.channels) {
                        state.fan_out(FadeEvent::ChannelCancelled {
                            group: target.key.group,
                            channel: target.key.channel,
                            parameter: target.key.parameter,
                        });
                    }
                    state.cancel_all_in_place(reason);
                    tick_interval = None;
                    state.fan_out(FadeEvent::FadeAborted);
                    continue;
                }
                connection.if_current(|| {
                    for key in completed_targets {
                        state.channels.retain(|target| target.key != key);
                        state.fan_out(FadeEvent::ChannelCompleted {
                            group: key.group, channel: key.channel, parameter: key.parameter,
                        });
                    }
                    maybe_complete_fade(&mut tick_interval, &mut state, &mut fade_completed_emitted);
                }).await;
            }
        }
    }

    if state.is_active() || state.is_waiting_for_readiness() {
        state.cancel_all_in_place(RecallReadinessCancellation::ActorStopped);
        state.fan_out(FadeEvent::FadeAborted);
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

/**
 * @cc [owner:mixxorz,label:safety] recall-admission
 * A recall MUST be admitted only while the engine's fixed generation is current and a fresh LV1
 * snapshot reports `Connected`; rejection MUST leave existing targets and readiness state intact.
 * A targetless detached recall remains generation-checked but MAY skip the fresh snapshot because it
 * installs neither targets nor a readiness barrier.
 */
/**
 * @cc [owner:mixxorz,label:product;safety] overlap-and-same-scene
 * For `FinishActiveTargets`, an exact scene index/name match MUST finish every active target owned by
 * that scene without installing incoming targets. When no exact-scene target is active, each incoming
 * target MUST replace only the active target with the same group, channel, and parameter.
 * `OverrideMatchingTargets` MUST restart incoming keys for the full duration and leave omitted active
 * targets unchanged.
 */
/**
 * @cc [owner:mixxorz,label:safety;product] zero-duration-write
 * A zero-duration recall with targets MUST send all exact target values in one checked LV1 batch
 * before removing overlaps or publishing channel completion; a rejected batch MUST produce none of
 * those success effects.
 */
/**
 * @cc [owner:mixxorz,label:product;safety] recall-start-value-precedence
 * For each installed nonzero-duration target, an ungated recall MUST prefer the current interpolated
 * value of its matching active target over a fresh live value. While readiness is already active, it
 * MUST prefer the fresh live value; either path MUST fall back to the other source and then to the
 * configured target when a parameter value is unavailable.
 */
/**
 * @cc [owner:mixxorz,label:safety;product] targetless-zero-duration-readiness
 * A targetless detached recall MUST install no readiness barrier. A targetless recall with an owned
 * completion MUST install readiness after connected-snapshot validation. A successful zero-duration
 * write MUST install readiness exactly when completion is owned.
 */
async fn handle_recall_scene_fade(
    connection: &Lv1Connection,
    state: &mut EngineState,
    config: crate::fade::types::FadeConfig,
    same_scene_behavior: SameSceneRecallBehavior,
    readiness: RecallReadinessRequest,
) -> Result<RecallSceneFadeOutcome, AppCommandError> {
    connection.ensure_current().await?;
    let completion_owned = readiness.completion.is_some();
    if config.targets.is_empty() && !completion_owned {
        return Ok(RecallSceneFadeOutcome::Started);
    }
    let snapshot = connection
        .request(|reply| Lv1Command::GetState { reply })
        .await?;
    if snapshot.connection != crate::lv1::ConnectionStatus::Connected {
        return Err(AppCommandError::Lv1Unavailable);
    }

    let now = Instant::now();
    let duration = Duration::from_millis(config.duration_ms);

    if config.targets.is_empty() {
        start_readiness_barrier(
            state,
            config.scene.index,
            config.scene.name,
            snapshot.ping_sequence,
            now,
            readiness,
        );
        return Ok(RecallSceneFadeOutcome::Started);
    }

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
        send_batch(connection, &state.event_bus, writes).await?;

        return connection.if_current(|| {
        for target in &config.targets {
            state.channels.retain(|ch| ch.key != target.key());
            state.fan_out(FadeEvent::ChannelCompleted {
                group: target.group,
                channel: target.channel,
                parameter: target.parameter,
            });
            tracing::debug!(event = "fade_channel_completed", group = target.group, channel = target.channel, parameter = ?target.parameter, "Fade channel completed: group {}, channel {}", target.group, target.channel);
        }
        if completion_owned {
            start_readiness_barrier(
                state,
                config.scene.index,
                config.scene.name,
                snapshot.ping_sequence,
                now,
                readiness,
            );
        }
        RecallSceneFadeOutcome::Started
        }).await.ok_or(AppCommandError::StaleGeneration);
    }

    let scene_owns_active_targets = state
        .channels
        .iter()
        .any(|active| active.scene == config.scene);
    let overriding_target_count = if same_scene_behavior
        == SameSceneRecallBehavior::OverrideMatchingTargets
        && scene_owns_active_targets
    {
        state
            .channels
            .iter()
            .filter(|active| {
                config
                    .targets
                    .iter()
                    .any(|target| active.key == target.key())
            })
            .count()
    } else {
        0
    };
    let finishing_target_count = match same_scene_behavior {
        SameSceneRecallBehavior::FinishActiveTargets => {
            state.finish_scene_on_next_tick(&config.scene)
        }
        SameSceneRecallBehavior::OverrideMatchingTargets => 0,
    };
    let outcome = if finishing_target_count > 0 {
        RecallSceneFadeOutcome::Finishing {
            target_count: finishing_target_count,
        }
    } else {
        for target in &config.targets {
            let active_start_value =
                state
                    .channels
                    .iter()
                    .find(|ch| ch.key == target.key())
                    .map(|ch| {
                        if ch.is_done(now) {
                            ch.target_value
                        } else {
                            ch.value_at(now)
                        }
                    });
            let snapshot_start_value = snapshot
                .channels
                .iter()
                .find(|ch| ch.group == target.group && ch.channel == target.channel)
                .and_then(|ch| live_value_for_snapshot(ch, target));
            let start_value = if state.is_waiting_for_readiness() {
                snapshot_start_value.or(active_start_value)
            } else {
                active_start_value.or(snapshot_start_value)
            }
            .unwrap_or(target.target);

            state.channels.retain(|ch| ch.key != target.key());
            state.channels.push(ActiveTarget::new(ActiveTargetInit {
                scene: config.scene.clone(),
                key: target.key(),
                start_value,
                target_value: target.target,
                curve: config.curve,
                duration,
                started_at: now,
            }));
        }
        if overriding_target_count > 0 {
            RecallSceneFadeOutcome::Overriding {
                target_count: overriding_target_count,
            }
        } else {
            RecallSceneFadeOutcome::Started
        }
    };

    start_readiness_barrier(
        state,
        config.scene.index,
        config.scene.name,
        snapshot.ping_sequence,
        now,
        readiness,
    );

    Ok(outcome)
}

/// @cc [owner:mixxorz,label:safety] readiness-only-admission
/// A readiness-only request MUST install or replace a barrier only after a generation-checked fresh
/// LV1 snapshot reports `Connected`; admission failure MUST preserve the existing barrier and active
/// targets.
async fn handle_wait_for_recall_readiness(
    connection: &Lv1Connection,
    state: &mut EngineState,
    scene: crate::fade::types::FadeSceneIdentity,
    readiness: RecallReadinessRequest,
) -> Result<(), AppCommandError> {
    let snapshot = connection
        .request(|reply| Lv1Command::GetState { reply })
        .await?;
    if snapshot.connection != crate::lv1::ConnectionStatus::Connected {
        return Err(AppCommandError::Lv1Unavailable);
    }
    start_readiness_barrier(
        state,
        scene.index,
        scene.name,
        snapshot.ping_sequence,
        Instant::now(),
        readiness,
    );
    Ok(())
}

fn start_readiness_barrier(
    state: &mut EngineState,
    scene_index: i32,
    scene_name: String,
    ping_sequence: u64,
    now: Instant,
    readiness: RecallReadinessRequest,
) {
    let generation = state.generation();
    let readiness_action = if state.is_waiting_for_readiness() {
        "reset"
    } else {
        "started"
    };
    state.start_or_reset_readiness(
        generation,
        scene_index,
        scene_name.clone(),
        ping_sequence,
        now,
        readiness,
    );
    tracing::debug!(
        event = "fade_post_recall_ping_barrier",
        action = readiness_action,
        generation,
        scene_index,
        scene_name = %scene_name,
        "Fade readiness barrier {readiness_action} after scene recall"
    );
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

/// @cc [owner:mixxorz,label:safety;reliability] checked-batch-failure-publication
/// Every fade write batch MUST pass through the generation-fenced LV1 connection. A non-staleness
/// failure MUST publish exactly one `WriteFailed` fact and a complete user-facing error message if
/// the engine's generation remains current at publication. If it is stale by that point, the engine
/// MUST publish and log nothing.
async fn send_batch(
    connection: &Lv1Connection,
    event_bus: &AppEventBus,
    writes: Vec<Lv1ParameterWrite>,
) -> Result<(), AppCommandError> {
    let result = connection.send(Lv1Command::WriteBatch(writes)).await;
    if let Err(error) = &result
        && *error != AppCommandError::StaleGeneration
    {
        connection.if_current(|| {
            let reason = error.to_string();
            tracing::error!(event = "fade_write_failed", reason = %reason, "Fade write failed: {reason}");
            event_bus.publish_fade(connection.generation(), FadeEvent::WriteFailed { reason });
        }).await;
    }
    result
}

/// @cc [owner:mixxorz,label:safety;product] pan-family-manual-override
/// A confirmed manual pan intervention MUST cancel all Pan, Balance, and Width targets for that
/// group/channel but MUST NOT cancel its fader target. An active Pan target requires consecutive
/// out-of-threshold reports. If that group/channel has Balance or Width targets but no Pan target, a
/// pan report MUST cancel those targets immediately. Removing the final active target MUST publish
/// terminal fade completion.
fn handle_pan_family_pan_report(
    state: &mut EngineState,
    group: i32,
    channel: i32,
    reported_pan: f64,
    tick_interval: &mut Option<tokio::time::Interval>,
    fade_completed_emitted: &mut bool,
) {
    let pan_override = if let Some(pan_target) = state.channels.iter_mut().find(|ch| {
        ch.key.group == group && ch.key.channel == channel && ch.key.parameter == FadeParameter::Pan
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
            ch.key.group == group && ch.key.channel == channel && ch.key.parameter.is_pan_family()
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
            ch.key.group == group && ch.key.channel == channel && ch.key.parameter.is_pan_family();
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
    use crate::fade::FadeEngineHandle;
    use crate::fade::curve::FadeCurve;
    use crate::fade::types::{FadeConfig, FadeSceneIdentity, FadeTarget};
    use crate::fade::{RecallReadinessError, SameSceneRecallBehavior};
    use crate::lv1::{
        ConnectionStatus, Lv1Command, Lv1Event, Lv1ParameterWrite, Lv1StateSnapshot,
        Lv1WriteParameter, test_actor_handle,
    };
    use crate::runtime::errors::AppCommandError;
    use crate::runtime::events::{AppEventBus, RuntimeLifecycleEvent};
    use crate::test_support::TracingCapture;
    use tokio::sync::oneshot;
    use tracing::Level;

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

    async fn spawn_runtime_for_test() -> (
        AppEventBus,
        FadeEngineHandle,
        tokio::sync::mpsc::Receiver<Lv1Command>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let event_bus = AppEventBus::default();
        let lv1 = test_actor_handle(tx);
        let runtime_generation = RuntimeGeneration::new();
        let (engine, task) = build_engine(runtime_generation, event_bus.clone(), 0, lv1);
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
        start_fade_with_behavior(engine, config, SameSceneRecallBehavior::FinishActiveTargets).await
    }

    async fn start_fade_with_behavior(
        engine: &FadeEngineHandle,
        config: FadeConfig,
        same_scene_behavior: SameSceneRecallBehavior,
    ) -> Result<(), AppCommandError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        engine
            .send(FadeCommand::RecallSceneFade {
                config,
                same_scene_behavior,
                readiness: RecallReadinessRequest::detached(
                    Instant::now() + Duration::from_secs(5),
                ),
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
        spawn_runtime_for_ping_gate_test_with_bus(AppEventBus::default(), snapshots).await
    }

    async fn spawn_runtime_for_ping_gate_test_with_bus(
        event_bus: AppEventBus,
        snapshots: Vec<Lv1StateSnapshot>,
    ) -> (
        AppEventBus,
        FadeEngineHandle,
        tokio::sync::mpsc::Receiver<Vec<Lv1ParameterWrite>>,
    ) {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let lv1 = test_actor_handle(tx);
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(7).await;
        let (engine, task) = build_engine(runtime_generation, event_bus.clone(), 7, lv1);
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

    fn publish_ping(event_bus: &AppEventBus, generation: u64, sequence: u64) {
        event_bus.publish(AppEvent::Lv1 {
            generation,
            event: Lv1Event::PingReceived { sequence },
        });
    }

    async fn next_write_batch(
        write_rx: &mut tokio::sync::mpsc::Receiver<Vec<Lv1ParameterWrite>>,
    ) -> Vec<Lv1ParameterWrite> {
        tokio::time::timeout(Duration::from_secs(1), write_rx.recv())
            .await
            .expect("fade write should arrive")
            .expect("LV1 write channel should remain open")
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
            match events.recv().await {
                Ok(AppEvent::Fade {
                    generation: 7,
                    event: FadeEvent::FadeAborted,
                }) => return,
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    panic!("event bus should remain open")
                }
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

    async fn start_owned_readiness(
        engine: &FadeEngineHandle,
        deadline: Instant,
    ) -> oneshot::Receiver<Result<(), RecallReadinessError>> {
        let (completion, completed) = oneshot::channel();
        let (reply, accepted) = oneshot::channel();
        engine
            .send(FadeCommand::WaitForRecallReadiness {
                scene: scene(2, "Verse"),
                readiness: RecallReadinessRequest {
                    deadline,
                    completion: Some(completion),
                },
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(accepted.await.unwrap(), Ok(()));
        completed
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
    async fn pan_override_requires_consecutive_deviations_and_resets_after_matching_report() {
        let (event_bus, engine, mut commands) = spawn_runtime_for_test().await;
        let mut events = event_bus.subscribe();
        tokio::spawn(async move {
            while let Some(command) = commands.recv().await {
                match command {
                    Lv1Command::GetState { reply } => {
                        let _ = reply.send(connected_snapshot(0, vec![]));
                    }
                    Lv1Command::WriteBatch(_) => {}
                    _ => panic!("unexpected LV1 command"),
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
                        target: 0.0,
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
                        target: 1.4,
                    },
                    FadeTarget {
                        group: 0,
                        channel: 1,
                        parameter: FadeParameter::Pan,
                        target: 45.0,
                    },
                ],
                10_000,
            ),
        )
        .await
        .unwrap();
        while events.try_recv().is_ok() {}

        for pan in [45.0, 0.0, 45.0] {
            event_bus.publish_lv1(
                0,
                Lv1Event::PanChanged {
                    group: 0,
                    channel: 0,
                    pan,
                },
            );
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !std::iter::from_fn(|| events.try_recv().ok()).any(|event| matches!(
                event,
                AppEvent::Fade {
                    event: FadeEvent::ChannelCancelled {
                        group: 0,
                        channel: 0,
                        ..
                    },
                    ..
                }
            ))
        );

        event_bus.publish_lv1(
            0,
            Lv1Event::PanChanged {
                group: 0,
                channel: 0,
                pan: 45.0,
            },
        );
        let mut observed = Vec::new();
        tokio::time::timeout(Duration::from_secs(1), async {
            while observed
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        AppEvent::Fade {
                            event: FadeEvent::ChannelCancelled {
                                group: 0,
                                channel: 0,
                                ..
                            },
                            ..
                        }
                    )
                })
                .count()
                < 3
            {
                observed.push(events.recv().await.unwrap());
            }
        })
        .await
        .expect("pan-family cancellation events should arrive");
        for parameter in [
            FadeParameter::Pan,
            FadeParameter::Balance,
            FadeParameter::Width,
        ] {
            assert!(observed.iter().any(|event| matches!(
                event,
                AppEvent::Fade { event: FadeEvent::ChannelCancelled { group: 0, channel: 0, parameter: cancelled }, .. } if *cancelled == parameter
            )));
        }
        assert!(observed.iter().any(|event| matches!(
            event,
            AppEvent::Fade {
                event: FadeEvent::ChannelOverride {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::Pan
                },
                ..
            }
        )));
        assert!(!observed.iter().any(|event| matches!(
            event,
            AppEvent::Fade {
                event: FadeEvent::ChannelCancelled { channel: 1, .. } | FadeEvent::FadeCompleted,
                ..
            }
        )));
    }

    #[tokio::test]
    async fn pan_report_cancelling_balance_and_width_completes_the_fade() {
        let (event_bus, engine, mut commands) = spawn_runtime_for_test().await;
        let mut events = event_bus.subscribe();
        tokio::spawn(async move {
            while let Some(command) = commands.recv().await {
                match command {
                    Lv1Command::GetState { reply } => {
                        let _ = reply.send(connected_snapshot(0, vec![]));
                    }
                    Lv1Command::WriteBatch(_) => {}
                    _ => panic!("unexpected LV1 command"),
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
                        parameter: FadeParameter::Balance,
                        target: 45.0,
                    },
                    FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::Width,
                        target: 1.4,
                    },
                ],
                10_000,
            ),
        )
        .await
        .unwrap();
        while events.try_recv().is_ok() {}

        event_bus.publish_lv1(
            0,
            Lv1Event::PanChanged {
                group: 0,
                channel: 0,
                pan: 45.0,
            },
        );
        let observed = tokio::time::timeout(Duration::from_secs(1), async {
            let mut observed = Vec::new();
            loop {
                let event = events.recv().await.unwrap();
                let completed = matches!(
                    event,
                    AppEvent::Fade {
                        event: FadeEvent::FadeCompleted,
                        ..
                    }
                );
                observed.push(event);
                if completed {
                    break observed;
                }
            }
        })
        .await
        .expect("last pan-family cancellation should complete the fade");

        for parameter in [FadeParameter::Balance, FadeParameter::Width] {
            assert!(observed.iter().any(|event| matches!(
                event,
                AppEvent::Fade { event: FadeEvent::ChannelCancelled { group: 0, channel: 0, parameter: cancelled }, .. } if *cancelled == parameter
            )));
        }
        assert!(!observed.iter().any(|event| matches!(
            event,
            AppEvent::Fade {
                event: FadeEvent::ChannelCancelled {
                    parameter: FadeParameter::Pan,
                    ..
                },
                ..
            }
        )));
    }

    #[tokio::test]
    async fn zero_duration_fade_sends_all_parameters_in_one_batch() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let event_bus = AppEventBus::default();
        let lv1 = test_actor_handle(tx);
        let runtime_generation = RuntimeGeneration::new();
        let (engine, task) = build_engine(runtime_generation, event_bus.clone(), 0, lv1);
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

    struct ConnectionFixture {
        authority: RuntimeGeneration,
        bus: AppEventBus,
        events: tokio::sync::broadcast::Receiver<AppEvent>,
        engine: FadeEngineHandle,
        lv1: Lv1ActorHandle,
        commands: tokio::sync::mpsc::Receiver<Lv1Command>,
    }

    impl ConnectionFixture {
        async fn new() -> Self {
            let authority = RuntimeGeneration::new();
            authority.set(7).await;
            let bus = AppEventBus::default();
            let events = bus.subscribe();
            let (tx, commands) = tokio::sync::mpsc::channel(1);
            let lv1 = test_actor_handle(tx);
            let (engine, task) = build_engine(authority.clone(), bus.clone(), 7, lv1.clone());
            task.spawn();
            Self {
                authority,
                bus,
                events,
                engine,
                lv1,
                commands,
            }
        }

        async fn request(
            &self,
            readiness_only: bool,
            duration_ms: u64,
        ) -> oneshot::Receiver<Result<(), AppCommandError>> {
            let (reply, response) = oneshot::channel();
            let readiness =
                RecallReadinessRequest::detached(Instant::now() + Duration::from_secs(5));
            let command = if readiness_only {
                FadeCommand::WaitForRecallReadiness {
                    scene: scene(1, "Intro"),
                    readiness,
                    reply: Some(reply),
                }
            } else {
                FadeCommand::RecallSceneFade {
                    config: fade_config(
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
                                target: 25.0,
                            },
                            FadeTarget {
                                group: 0,
                                channel: 0,
                                parameter: FadeParameter::Width,
                                target: 1.2,
                            },
                        ],
                        duration_ms,
                    ),
                    same_scene_behavior: SameSceneRecallBehavior::FinishActiveTargets,
                    readiness,
                    reply: Some(reply),
                }
            };
            self.engine.send(command).await.unwrap();
            response
        }

        async fn snapshot_request(&mut self) -> oneshot::Sender<Lv1StateSnapshot> {
            match tokio::time::timeout(Duration::from_secs(1), self.commands.recv())
                .await
                .unwrap()
                .unwrap()
            {
                Lv1Command::GetState { reply } => reply,
                _ => panic!("expected an LV1 snapshot request"),
            }
        }

        async fn fill_mailbox(&self) {
            self.lv1.send(Lv1Command::WriteBatch(vec![])).await.unwrap();
        }

        async fn release_mailbox(&mut self, close: bool) {
            if close {
                self.commands.close();
            }
            assert!(
                matches!(self.commands.recv().await, Some(Lv1Command::WriteBatch(batch)) if batch.is_empty())
            );
        }

        fn release_readiness(&self) {
            publish_ping(&self.bus, 7, 1);
            publish_ping(&self.bus, 7, 2);
        }

        async fn terminal_events(&mut self) -> Vec<FadeEvent> {
            tokio::time::timeout(Duration::from_secs(1), async {
                let mut result = Vec::new();
                loop {
                    if let AppEvent::Fade { generation, event } = self.events.recv().await.unwrap()
                    {
                        assert_eq!(generation, 7);
                        let terminal =
                            matches!(event, FadeEvent::FadeAborted | FadeEvent::FadeCompleted);
                        result.push(event);
                        if terminal {
                            return result;
                        }
                    }
                }
            })
            .await
            .unwrap()
        }
    }

    #[tokio::test]
    async fn old_engine_cannot_adopt_the_current_connections_generation() {
        let mut fixture = ConnectionFixture::new().await;
        fixture.authority.advance().await;
        for readiness_only in [false, true] {
            assert_eq!(
                fixture.request(readiness_only, 0).await.await.unwrap(),
                Err(AppCommandError::StaleGeneration)
            );
        }
        assert!(fixture.commands.try_recv().is_err());
    }

    #[tokio::test]
    async fn generation_flip_while_lv1_snapshot_is_pending_is_rejected_after_snapshot() {
        for readiness_only in [false, true] {
            let mut fixture = ConnectionFixture::new().await;
            let result = fixture.request(readiness_only, 0).await;
            let reply = fixture.snapshot_request().await;
            fixture.authority.advance().await;
            reply.send(connected_snapshot(0, vec![])).unwrap();
            assert_eq!(result.await.unwrap(), Err(AppCommandError::StaleGeneration));
            assert!(fixture.commands.try_recv().is_err());
        }
    }

    #[tokio::test]
    async fn pending_snapshot_admission_cannot_cross_generations() {
        for readiness_only in [false, true] {
            for close in [false, true] {
                let mut fixture = ConnectionFixture::new().await;
                fixture.fill_mailbox().await;
                let result = fixture.request(readiness_only, 0).await;
                tokio::task::yield_now().await;
                fixture.authority.advance().await;
                fixture.release_mailbox(close).await;
                assert_eq!(result.await.unwrap(), Err(AppCommandError::StaleGeneration));
                assert!(fixture.commands.try_recv().is_err());
            }
        }
    }

    #[tokio::test]
    async fn closed_snapshot_reply_reports_staleness_only_after_revocation() {
        for readiness_only in [false, true] {
            for revoked in [false, true] {
                let mut fixture = ConnectionFixture::new().await;
                let result = fixture.request(readiness_only, 0).await;
                let reply = fixture.snapshot_request().await;
                if revoked {
                    fixture.authority.advance().await;
                }
                drop(reply);
                let expected = if revoked {
                    AppCommandError::StaleGeneration
                } else {
                    AppCommandError::ReplyChannelClosed
                };
                assert_eq!(result.await.unwrap(), Err(expected));
                assert!(fixture.commands.try_recv().is_err());
            }
        }
    }

    #[tokio::test]
    async fn same_generation_disconnected_snapshot_blocks_zero_duration_recall_write() {
        let mut fixture = ConnectionFixture::new().await;
        let result = fixture.request(false, 0).await;
        let mut snapshot = connected_snapshot(0, vec![]);
        snapshot.connection = ConnectionStatus::Disconnected;
        fixture.snapshot_request().await.send(snapshot).unwrap();
        assert_eq!(result.await.unwrap(), Err(AppCommandError::Lv1Unavailable));
        assert!(fixture.commands.try_recv().is_err());
    }

    #[tokio::test]
    async fn zero_duration_write_waiting_for_capacity_is_rejected_after_revocation() {
        let capture = TracingCapture::new();
        let _guard = capture.install();
        for close in [false, true] {
            let mut fixture = ConnectionFixture::new().await;
            let result = fixture.request(false, 0).await;
            let reply = fixture.snapshot_request().await;
            fixture.fill_mailbox().await;
            reply.send(connected_snapshot(0, vec![])).unwrap();
            tokio::task::yield_now().await;
            fixture.authority.advance().await;
            fixture.release_mailbox(close).await;
            assert_eq!(result.await.unwrap(), Err(AppCommandError::StaleGeneration));
            assert!(fixture.commands.try_recv().is_err());
            assert!(
                !std::iter::from_fn(|| fixture.events.try_recv().ok()).any(|event| matches!(
                    event,
                    AppEvent::Fade {
                        event: FadeEvent::ChannelCompleted { .. }
                            | FadeEvent::FadeCompleted
                            | FadeEvent::WriteFailed { .. },
                        ..
                    }
                ))
            );
        }
        assert!(
            capture
                .matching("fade_write_failed", Level::ERROR)
                .is_empty()
        );
        assert!(capture.matching("fade_completed", Level::INFO).is_empty());
    }

    async fn assert_failed_tick(revoke: bool, close: bool) {
        let capture = TracingCapture::new();
        let _guard = capture.install();
        let mut fixture = ConnectionFixture::new().await;
        let result = fixture.request(false, 100).await;
        fixture
            .snapshot_request()
            .await
            .send(connected_snapshot(0, vec![]))
            .unwrap();
        assert_eq!(result.await.unwrap(), Ok(()));
        fixture.fill_mailbox().await;
        fixture.release_readiness();
        tokio::task::yield_now().await;
        assert_eq!(
            capture
                .matching("fade_post_recall_ping_released", Level::DEBUG)
                .len(),
            1
        );
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
        if revoke {
            fixture.authority.advance().await;
        }
        fixture.release_mailbox(close).await;
        let events = fixture.terminal_events().await;
        assert!(
            events
                .iter()
                .any(|event| matches!(event, FadeEvent::FadeAborted))
        );
        assert!(!events.iter().any(|event| matches!(
            event,
            FadeEvent::ChannelCompleted { .. } | FadeEvent::FadeCompleted
        )));
        for parameter in [
            FadeParameter::FaderDb,
            FadeParameter::Pan,
            FadeParameter::Width,
        ] {
            assert!(events.iter().any(|event| matches!(event, FadeEvent::ChannelCancelled { parameter: cancelled, .. } if *cancelled == parameter)));
        }
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, FadeEvent::WriteFailed { .. }))
                .count(),
            usize::from(!revoke)
        );
        assert_eq!(
            capture.matching("fade_write_failed", Level::ERROR).len(),
            usize::from(!revoke)
        );
        assert!(capture.matching("fade_completed", Level::INFO).is_empty());
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(fixture.commands.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn timed_write_waiting_for_capacity_is_rejected_after_revocation() {
        assert_failed_tick(true, false).await;
    }

    #[tokio::test(start_paused = true)]
    async fn stale_generation_failed_reservation_is_silent() {
        assert_failed_tick(true, true).await;
    }

    #[tokio::test(start_paused = true)]
    async fn current_write_failure_cancels_all_targets_without_reporting_completion() {
        assert_failed_tick(false, true).await;
    }

    #[tokio::test(start_paused = true)]
    async fn same_connection_batch_preserves_parameter_order_and_completes_once() {
        let mut fixture = ConnectionFixture::new().await;
        let result = fixture.request(false, 100).await;
        fixture
            .snapshot_request()
            .await
            .send(connected_snapshot(0, vec![]))
            .unwrap();
        assert_eq!(result.await.unwrap(), Ok(()));
        fixture.release_readiness();
        tokio::time::advance(Duration::from_millis(100)).await;
        let command = fixture.commands.recv().await.unwrap();
        let Lv1Command::WriteBatch(batch) = command else {
            panic!("expected final values");
        };
        assert!(
            batch
                .iter()
                .all(|write| write.group == 0 && write.channel == 0)
        );
        assert_eq!(
            batch
                .into_iter()
                .map(|write| (write.parameter, write.value))
                .collect::<Vec<_>>(),
            vec![
                (Lv1WriteParameter::FaderDb, -12.5),
                (Lv1WriteParameter::Pan, 25.0),
                (Lv1WriteParameter::Width, 1.2),
            ]
        );
        let events = fixture.terminal_events().await;
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, FadeEvent::ChannelCompleted { .. }))
                .count(),
            3
        );
        assert!(matches!(events.last(), Some(FadeEvent::FadeCompleted)));
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(fixture.commands.try_recv().is_err());
        assert!(
            !std::iter::from_fn(|| fixture.events.try_recv().ok()).any(|event| matches!(
                event,
                AppEvent::Fade {
                    event: FadeEvent::FadeCompleted,
                    ..
                }
            ))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn readiness_only_command_completes_after_two_newer_pings() {
        let (event_bus, engine, mut writes) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(10, vec![])]).await;
        let (completion, mut completed) = oneshot::channel();
        let (reply, accepted) = oneshot::channel();

        engine
            .send(FadeCommand::WaitForRecallReadiness {
                scene: scene(2, "Verse"),
                readiness: RecallReadinessRequest {
                    deadline: Instant::now() + Duration::from_secs(5),
                    completion: Some(completion),
                },
                reply: Some(reply),
            })
            .await
            .unwrap();

        assert_eq!(accepted.await.unwrap(), Ok(()));
        publish_ping(&event_bus, 7, 11);
        assert!(completed.try_recv().is_err());
        publish_ping(&event_bus, 7, 12);
        assert_eq!(completed.await.unwrap(), Ok(()));
        assert!(writes.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn newer_readiness_command_cancels_previous_completion_as_superseded() {
        let (_event_bus, engine, _writes) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(10, vec![]),
            connected_snapshot(11, vec![]),
        ])
        .await;
        let (first_completion, first_completed) = oneshot::channel();
        let (first_reply, first_accepted) = oneshot::channel();
        let (second_reply, second_accepted) = oneshot::channel();

        engine
            .send(FadeCommand::WaitForRecallReadiness {
                scene: scene(1, "Intro"),
                readiness: RecallReadinessRequest {
                    deadline: Instant::now() + Duration::from_secs(5),
                    completion: Some(first_completion),
                },
                reply: Some(first_reply),
            })
            .await
            .unwrap();
        assert_eq!(first_accepted.await.unwrap(), Ok(()));

        engine
            .send(FadeCommand::WaitForRecallReadiness {
                scene: scene(2, "Verse"),
                readiness: RecallReadinessRequest::detached(
                    Instant::now() + Duration::from_secs(5),
                ),
                reply: Some(second_reply),
            })
            .await
            .unwrap();

        assert_eq!(second_accepted.await.unwrap(), Ok(()));
        assert_eq!(
            first_completed.await.unwrap(),
            Err(RecallReadinessError::Cancelled(
                RecallReadinessCancellation::Superseded,
            )),
        );
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn readiness_only_command_uses_its_absolute_deadline() {
        let (_event_bus, engine, _writes) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(10, vec![])]).await;
        let (completion, completed) = oneshot::channel();
        let (reply, accepted) = oneshot::channel();

        engine
            .send(FadeCommand::WaitForRecallReadiness {
                scene: scene(2, "Verse"),
                readiness: RecallReadinessRequest {
                    deadline: Instant::now() + Duration::from_millis(200),
                    completion: Some(completion),
                },
                reply: Some(reply),
            })
            .await
            .unwrap();

        assert_eq!(accepted.await.unwrap(), Ok(()));
        tokio::time::advance(Duration::from_millis(200)).await;
        assert_eq!(
            completed.await.unwrap(),
            Err(RecallReadinessError::TimedOut {
                generation: 7,
                scene_index: 2,
                scene_name: "Verse".to_string(),
                observed_ping_count: 0,
            }),
        );
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn pings_after_readiness_deadline_time_out_without_releasing_or_writing() {
        let (event_bus, engine, mut writes) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(
                10,
                vec![channel_info(0, -20.0, None)],
            )])
            .await;
        let (completion, completed) = oneshot::channel();
        let (reply, accepted) = oneshot::channel();

        engine
            .send(FadeCommand::RecallSceneFade {
                config: fade_config(
                    scene(2, "Verse"),
                    vec![FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::FaderDb,
                        target: -10.0,
                    }],
                    1_000,
                ),
                same_scene_behavior: SameSceneRecallBehavior::FinishActiveTargets,
                readiness: RecallReadinessRequest {
                    deadline: Instant::now() + Duration::from_millis(200),
                    completion: Some(completion),
                },
                reply: Some(reply),
            })
            .await
            .unwrap();

        assert_eq!(accepted.await.unwrap(), Ok(()));
        tokio::time::advance(Duration::from_millis(201)).await;
        publish_ping(&event_bus, 7, 11);
        publish_ping(&event_bus, 7, 12);
        assert_eq!(
            completed.await.unwrap(),
            Err(RecallReadinessError::TimedOut {
                generation: 7,
                scene_index: 2,
                scene_name: "Verse".to_string(),
                observed_ping_count: 0,
            }),
        );
        assert_no_write_after_cancellation(&mut writes).await;
    }

    #[tokio::test(start_paused = true)]
    async fn abort_all_cancels_readiness_completion() {
        let (_event_bus, engine, _writes) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(10, vec![])]).await;
        let completed =
            start_owned_readiness(&engine, Instant::now() + Duration::from_secs(5)).await;

        engine
            .send(FadeCommand::AbortAll { reply: None })
            .await
            .unwrap();

        assert_eq!(
            completed.await.unwrap(),
            Err(RecallReadinessError::Cancelled(
                RecallReadinessCancellation::Aborted,
            )),
        );
    }

    #[tokio::test(start_paused = true)]
    async fn disconnect_cancels_readiness_completion() {
        let (event_bus, engine, _writes) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(10, vec![])]).await;
        let completed =
            start_owned_readiness(&engine, Instant::now() + Duration::from_secs(5)).await;

        event_bus.publish_lv1(
            7,
            Lv1Event::Disconnected {
                reason: "test disconnect".to_string(),
            },
        );

        assert_eq!(
            completed.await.unwrap(),
            Err(RecallReadinessError::Cancelled(
                RecallReadinessCancellation::Disconnected,
            )),
        );
    }

    #[tokio::test(start_paused = true)]
    async fn generation_change_cancels_readiness_completion() {
        let (event_bus, engine, _writes) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(10, vec![])]).await;
        let completed =
            start_owned_readiness(&engine, Instant::now() + Duration::from_secs(5)).await;

        event_bus.publish(AppEvent::Runtime(
            RuntimeLifecycleEvent::ActiveGenerationChanged { generation: 8 },
        ));

        assert_eq!(
            completed.await.unwrap(),
            Err(RecallReadinessError::Cancelled(
                RecallReadinessCancellation::GenerationChanged,
            )),
        );
    }

    #[tokio::test(start_paused = true)]
    async fn actor_stop_cancels_readiness_completion() {
        let (_event_bus, engine, _writes) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(10, vec![])]).await;
        let completed =
            start_owned_readiness(&engine, Instant::now() + Duration::from_secs(5)).await;

        drop(engine);

        assert_eq!(
            completed.await.unwrap(),
            Err(RecallReadinessError::Cancelled(
                RecallReadinessCancellation::ActorStopped,
            )),
        );
    }

    #[tokio::test(start_paused = true)]
    async fn empty_recall_with_completion_waits_without_completing_the_fade() {
        let (event_bus, engine, _writes) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(10, vec![])]).await;
        let mut events = event_bus.subscribe();
        let (completion, completed) = oneshot::channel();
        let (reply, accepted) = oneshot::channel();

        engine
            .send(FadeCommand::RecallSceneFade {
                config: fade_config(scene(2, "Verse"), vec![], 1_000),
                same_scene_behavior: SameSceneRecallBehavior::FinishActiveTargets,
                readiness: RecallReadinessRequest {
                    deadline: Instant::now() + Duration::from_secs(5),
                    completion: Some(completion),
                },
                reply: Some(reply),
            })
            .await
            .unwrap();

        assert_eq!(accepted.await.unwrap(), Ok(()));
        assert!(events.try_recv().is_err());
        publish_ping(&event_bus, 7, 11);
        publish_ping(&event_bus, 7, 12);
        assert_eq!(completed.await.unwrap(), Ok(()));
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn post_recall_ping_timeout_aborts_fade_and_logs_warning() {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let (event_bus, engine, mut write_rx) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(
                40,
                vec![channel_info(0, -20.0, None)],
            )])
            .await;
        let mut events = event_bus.subscribe();

        start_fade(
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

        let timeout_warnings = captured.matching("fade_post_recall_ping_timeout", Level::WARN);
        assert_eq!(timeout_warnings.len(), 1);
        let warning = &timeout_warnings[0];
        assert_eq!(
            warning.message.as_deref(),
            Some(
                "Fades were aborted because LV1 did not resume its keepalive cadence after scene recall"
            )
        );
        assert_eq!(
            warning.fields.get("generation").map(String::as_str),
            Some("7")
        );
        assert_eq!(
            warning.fields.get("scene_index").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            warning.fields.get("scene_name").map(String::as_str),
            Some("Intro")
        );
        assert_eq!(
            warning
                .fields
                .get("observed_ping_count")
                .map(String::as_str),
            Some("0")
        );
        assert_eq!(
            warning.fields.get("timeout_ms").map(String::as_str),
            Some("5000")
        );
        for field in ["action", "target_count"] {
            assert!(
                !warning.fields.contains_key(field),
                "timeout warning unexpectedly included {field}"
            );
        }
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

        start_fade(
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

        start_fade(
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
        let (event_bus, engine, mut write_rx) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(40, vec![channel_info(0, -20.0, None)]),
            connected_snapshot(40, vec![channel_info(0, -20.0, None)]),
        ])
        .await;
        let mut events = event_bus.subscribe();

        start_fade(
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
        )
        .await
        .unwrap();

        start_fade(
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

        start_fade(
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

        start_fade(
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

        start_fade(
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

        start_fade(
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
        )
        .await
        .unwrap();

        tokio::time::advance(Duration::from_secs(4)).await;

        start_fade(
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

        start_fade(
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
        )
        .await
        .unwrap();
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 41 },
        });

        start_fade(
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

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn repeated_scene_recall_finishes_only_its_targets_after_readiness() {
        let scene_a_boundary = 40;
        let scene_b_boundary = 43;
        let repeated_recall_boundary = 46;
        let scene_b_exact_target = 0.0;
        let (event_bus, engine, mut write_rx) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(
                scene_a_boundary,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
            connected_snapshot(
                scene_b_boundary,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
            connected_snapshot(
                repeated_recall_boundary,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
        ])
        .await;
        let mut events = event_bus.subscribe();
        let scene_a_config = fade_config(
            scene(17, "Verse"),
            vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: -10.0,
            }],
            1_000,
        );

        start_fade(&engine, scene_a_config.clone())
            .await
            .expect("Scene A recall should validate");
        assert_no_write(&mut write_rx).await;
        publish_ping(&event_bus, 7, scene_a_boundary + 1);
        assert_no_write(&mut write_rx).await;
        publish_ping(&event_bus, 7, scene_a_boundary + 2);
        tokio::time::advance(Duration::from_millis(200)).await;
        let scene_a_running = next_write_batch(&mut write_rx).await;
        assert!(scene_a_running.iter().any(|write| write.channel == 1));
        while write_rx.try_recv().is_ok() {}

        start_fade(
            &engine,
            fade_config(
                scene(18, "Chorus"),
                vec![FadeTarget {
                    group: 0,
                    channel: 2,
                    parameter: FadeParameter::FaderDb,
                    target: scene_b_exact_target,
                }],
                1_000,
            ),
        )
        .await
        .expect("Scene B recall should validate");
        assert_no_write(&mut write_rx).await;
        publish_ping(&event_bus, 7, scene_b_boundary + 1);
        assert_no_write(&mut write_rx).await;
        publish_ping(&event_bus, 7, scene_b_boundary + 2);
        tokio::time::advance(Duration::from_millis(200)).await;
        let scene_b_running = next_write_batch(&mut write_rx).await;
        let scene_b_before_repeated_recall = scene_b_running
            .iter()
            .find(|write| write.channel == 2 && write.parameter == Lv1WriteParameter::FaderDb)
            .expect("Scene B should make progress after its own readiness gate releases")
            .value;
        assert!(
            scene_b_before_repeated_recall > -20.0
                && scene_b_before_repeated_recall < scene_b_exact_target
        );
        while write_rx.try_recv().is_ok() {}

        start_fade(&engine, scene_a_config)
            .await
            .expect("repeated Scene A recall should validate");
        assert_no_write(&mut write_rx).await;
        tokio::time::advance(Duration::from_millis(200)).await;
        assert_no_write(&mut write_rx).await;
        publish_ping(&event_bus, 7, repeated_recall_boundary + 1);
        assert_no_write(&mut write_rx).await;
        publish_ping(&event_bus, 7, repeated_recall_boundary + 2);
        for _ in 0..6 {
            tokio::task::yield_now().await;
        }

        let writes = next_write_batch(&mut write_rx).await;
        assert!(writes.contains(&Lv1ParameterWrite {
            group: 0,
            channel: 1,
            parameter: Lv1WriteParameter::FaderDb,
            value: -10.0,
        }));
        assert!(!writes.iter().any(|write| {
            write.channel == 2 && (write.value - scene_b_exact_target).abs() < 1e-10
        }));

        tokio::time::advance(Duration::from_millis(200)).await;
        tokio::task::yield_now().await;
        let resumed_writes = next_write_batch(&mut write_rx).await;
        let scene_b_resumed_value = resumed_writes
            .iter()
            .find(|write| write.channel == 2 && write.parameter == Lv1WriteParameter::FaderDb)
            .expect("Scene B should make progress after the repeated Scene A gate releases")
            .value;
        assert!(scene_b_resumed_value > scene_b_before_repeated_recall);
        assert!(scene_b_resumed_value < scene_b_exact_target);

        let completed_scene_a = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                match events.recv().await.expect("event bus should remain open") {
                    AppEvent::Fade {
                        event:
                            FadeEvent::ChannelCompleted {
                                group: 0,
                                channel: 1,
                                parameter: FadeParameter::FaderDb,
                            },
                        ..
                    } => return,
                    AppEvent::Fade {
                        event: FadeEvent::FadeCompleted,
                        ..
                    } => panic!("Scene B should keep the fade active"),
                    _ => {}
                }
            }
        })
        .await;
        assert!(completed_scene_a.is_ok(), "Scene A target should complete");
        tokio::task::yield_now().await;
        assert!(!matches!(
            events.try_recv(),
            Ok(AppEvent::Fade {
                event: FadeEvent::FadeCompleted,
                ..
            })
        ));
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn repeated_scene_recall_finishes_all_parameter_families_at_exact_targets() {
        let targets = [
            (FadeParameter::FaderDb, -10.0, Lv1WriteParameter::FaderDb),
            (FadeParameter::Pan, 12.0, Lv1WriteParameter::Pan),
            (FadeParameter::Balance, -11.0, Lv1WriteParameter::Balance),
            (FadeParameter::Width, 0.75, Lv1WriteParameter::Width),
        ];
        let (event_bus, engine, mut write_rx) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(40, vec![channel_info(1, -20.0, Some(0.0))]),
            connected_snapshot(42, vec![channel_info(1, -20.0, Some(0.0))]),
        ])
        .await;
        let repeated_scene_a = fade_config(
            scene(17, "Verse"),
            targets
                .iter()
                .map(|(parameter, target, _)| FadeTarget {
                    group: 0,
                    channel: 1,
                    parameter: *parameter,
                    target: *target,
                })
                .collect(),
            1_000,
        );

        start_fade(&engine, repeated_scene_a.clone())
            .await
            .expect("initial Scene A recall should validate");
        publish_ping(&event_bus, 7, 41);
        publish_ping(&event_bus, 7, 42);
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(250)).await;
        tokio::task::yield_now().await;
        while write_rx.try_recv().is_ok() {}

        start_fade(&engine, repeated_scene_a)
            .await
            .expect("repeated Scene A recall should validate");
        publish_ping(&event_bus, 7, 43);
        publish_ping(&event_bus, 7, 44);
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(200)).await;
        tokio::task::yield_now().await;

        let writes = next_write_batch(&mut write_rx).await;
        for (parameter, target, write_parameter) in targets {
            assert!(
                writes.contains(&Lv1ParameterWrite {
                    group: 0,
                    channel: 1,
                    parameter: write_parameter,
                    value: target,
                }),
                "{parameter:?} should finish at its exact target"
            );
        }
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn repeated_scene_override_replaces_from_current_value_without_immediate_finish() {
        let config = fade_config(
            scene(17, "Verse"),
            vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: 0.0,
            }],
            1_000,
        );
        let (event_bus, engine, mut write_rx) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(40, vec![channel_info(1, -20.0, None)]),
            connected_snapshot(42, vec![channel_info(1, -20.0, None)]),
        ])
        .await;

        start_fade(&engine, config.clone()).await.unwrap();
        publish_ping(&event_bus, 7, 41);
        publish_ping(&event_bus, 7, 42);
        tokio::time::advance(Duration::from_millis(250)).await;
        let before_override = next_write_batch(&mut write_rx).await;
        let before_value = before_override
            .iter()
            .find(|write| write.channel == 1)
            .expect("initial fade should be moving")
            .value;
        while write_rx.try_recv().is_ok() {}

        start_fade_with_behavior(
            &engine,
            config,
            SameSceneRecallBehavior::OverrideMatchingTargets,
        )
        .await
        .unwrap();
        tokio::task::yield_now().await;
        assert!(matches!(
            write_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        publish_ping(&event_bus, 7, 43);
        tokio::task::yield_now().await;
        assert!(matches!(
            write_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        publish_ping(&event_bus, 7, 44);
        tokio::time::advance(Duration::from_millis(100)).await;
        let after_override = next_write_batch(&mut write_rx).await;
        let after_value = after_override
            .iter()
            .find(|write| write.channel == 1)
            .expect("replacement fade should resume")
            .value;

        assert!(after_value >= before_value, "replacement must not rewind");
        assert!(after_value < 0.0, "replacement must not finish immediately");

        tokio::time::advance(Duration::from_millis(800)).await;
        let before_deadline = next_write_batch(&mut write_rx).await;
        assert!(before_deadline.iter().any(|write| {
            write.channel == 1 && write.parameter == Lv1WriteParameter::FaderDb && write.value < 0.0
        }));

        tokio::time::advance(Duration::from_millis(200)).await;
        let at_deadline = next_write_batch(&mut write_rx).await;
        assert!(at_deadline.contains(&Lv1ParameterWrite {
            group: 0,
            channel: 1,
            parameter: Lv1WriteParameter::FaderDb,
            value: 0.0,
        }));
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn repeated_scene_override_leaves_omitted_same_scene_target_active() {
        let initial = fade_config(
            scene(17, "Verse"),
            vec![
                FadeTarget {
                    group: 0,
                    channel: 1,
                    parameter: FadeParameter::FaderDb,
                    target: -10.0,
                },
                FadeTarget {
                    group: 0,
                    channel: 2,
                    parameter: FadeParameter::FaderDb,
                    target: -5.0,
                },
            ],
            1_000,
        );
        let reduced = fade_config(
            scene(17, "Verse"),
            vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: 0.0,
            }],
            1_000,
        );
        let (event_bus, engine, mut write_rx) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(
                40,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
            connected_snapshot(
                42,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
        ])
        .await;

        start_fade(&engine, initial).await.unwrap();
        publish_ping(&event_bus, 7, 41);
        publish_ping(&event_bus, 7, 42);
        tokio::time::advance(Duration::from_millis(200)).await;
        let _ = next_write_batch(&mut write_rx).await;
        while write_rx.try_recv().is_ok() {}

        start_fade_with_behavior(
            &engine,
            reduced,
            SameSceneRecallBehavior::OverrideMatchingTargets,
        )
        .await
        .unwrap();
        publish_ping(&event_bus, 7, 43);
        publish_ping(&event_bus, 7, 44);
        tokio::time::advance(Duration::from_millis(100)).await;
        let resumed = next_write_batch(&mut write_rx).await;
        assert!(resumed.iter().any(|write| write.channel == 1));
        assert!(resumed.iter().any(|write| write.channel == 2));
        assert!(
            !resumed
                .iter()
                .any(|write| write.channel == 2 && write.value == -5.0)
        );
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn repeated_scene_finish_uses_active_ownership_when_incoming_scope_changed() {
        let initial = fade_config(
            scene(17, "Verse"),
            vec![
                FadeTarget {
                    group: 0,
                    channel: 1,
                    parameter: FadeParameter::FaderDb,
                    target: -10.0,
                },
                FadeTarget {
                    group: 0,
                    channel: 2,
                    parameter: FadeParameter::FaderDb,
                    target: -5.0,
                },
            ],
            1_000,
        );
        let reduced = fade_config(
            scene(17, "Verse"),
            vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: 0.0,
            }],
            1_000,
        );
        let (event_bus, engine, mut write_rx) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(
                40,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
            connected_snapshot(
                42,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
        ])
        .await;

        start_fade(&engine, initial).await.unwrap();
        publish_ping(&event_bus, 7, 41);
        publish_ping(&event_bus, 7, 42);
        tokio::time::advance(Duration::from_millis(200)).await;
        let _ = next_write_batch(&mut write_rx).await;
        while write_rx.try_recv().is_ok() {}

        start_fade(&engine, reduced).await.unwrap();
        publish_ping(&event_bus, 7, 43);
        publish_ping(&event_bus, 7, 44);
        tokio::time::advance(Duration::from_millis(100)).await;
        let writes = next_write_batch(&mut write_rx).await;
        assert!(writes.contains(&Lv1ParameterWrite {
            group: 0,
            channel: 1,
            parameter: Lv1WriteParameter::FaderDb,
            value: -10.0,
        }));
        assert!(writes.contains(&Lv1ParameterWrite {
            group: 0,
            channel: 2,
            parameter: Lv1WriteParameter::FaderDb,
            value: -5.0,
        }));
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn repeated_scene_manual_override_cancels_only_the_repeated_scene_target() {
        let boundary = 42;
        let manual_value = -50.0;
        let scene_b_exact_target = 0.0;
        let (event_bus, engine, mut write_rx) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(
                40,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
            connected_snapshot(
                41,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
            connected_snapshot(
                boundary,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
        ])
        .await;
        let mut events = event_bus.subscribe();
        let repeated_scene_a = fade_config(
            scene(17, "Verse"),
            vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: -10.0,
            }],
            1_000,
        );

        start_fade(&engine, repeated_scene_a.clone())
            .await
            .expect("initial Scene A recall should validate");

        let scene_b = fade_config(
            scene(18, "Chorus"),
            vec![FadeTarget {
                group: 0,
                channel: 2,
                parameter: FadeParameter::FaderDb,
                target: scene_b_exact_target,
            }],
            1_000,
        );
        start_fade(&engine, scene_b)
            .await
            .expect("Scene B recall should validate");

        start_fade(&engine, repeated_scene_a)
            .await
            .expect("repeated Scene A recall should validate");
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::FaderChanged {
                group: 0,
                channel: 1,
                gain_db: manual_value,
            },
        });
        publish_ping(&event_bus, 7, boundary + 1);
        publish_ping(&event_bus, 7, boundary + 2);
        for _ in 0..6 {
            tokio::task::yield_now().await;
        }

        while let Ok(writes) = write_rx.try_recv() {
            assert!(!writes.iter().any(|write| {
                write.group == 0
                    && write.channel == 1
                    && write.parameter == Lv1WriteParameter::FaderDb
            }));
        }

        tokio::time::advance(Duration::from_millis(200)).await;
        tokio::task::yield_now().await;
        let scene_b_progress = next_write_batch(&mut write_rx).await;
        assert!(scene_b_progress.iter().any(|write| {
            write.group == 0
                && write.channel == 2
                && write.parameter == Lv1WriteParameter::FaderDb
                && (write.value - scene_b_exact_target).abs() >= 1e-10
        }));
        assert!(!scene_b_progress.iter().any(|write| {
            write.group == 0 && write.channel == 1 && write.parameter == Lv1WriteParameter::FaderDb
        }));

        let mut saw_scene_a_override = false;
        let mut saw_scene_a_cancelled = false;
        while let Ok(event) = events.try_recv() {
            match event {
                AppEvent::Fade {
                    event:
                        FadeEvent::ChannelOverride {
                            group: 0,
                            channel: 1,
                            parameter: FadeParameter::FaderDb,
                        },
                    ..
                } => saw_scene_a_override = true,
                AppEvent::Fade {
                    event:
                        FadeEvent::ChannelCancelled {
                            group: 0,
                            channel: 1,
                            parameter: FadeParameter::FaderDb,
                        },
                    ..
                } => saw_scene_a_cancelled = true,
                AppEvent::Fade {
                    event: FadeEvent::FadeCompleted,
                    ..
                } => panic!("the unrelated Scene B target should remain active"),
                _ => {}
            }
        }
        assert!(saw_scene_a_override);
        assert!(saw_scene_a_cancelled);
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

        start_fade(
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

        start_fade(
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

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn post_recall_ping_lag_keeps_gate_closed_until_timeout() {
        let (event_bus, engine, mut write_rx) = spawn_runtime_for_ping_gate_test_with_bus(
            AppEventBus::new(1),
            vec![connected_snapshot(40, vec![channel_info(0, -20.0, None)])],
        )
        .await;
        let mut events = event_bus.subscribe();

        start_fade(
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
        )
        .await
        .unwrap();

        for sequence in 41..=48 {
            event_bus.publish_lv1(7, Lv1Event::PingReceived { sequence });
        }
        tokio::task::yield_now().await;
        event_bus.publish_lv1(7, Lv1Event::PingReceived { sequence: 49 });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(40)).await;
        assert_no_write(&mut write_rx).await;

        tokio::time::advance(Duration::from_millis(4_960)).await;
        wait_for_fade_aborted(&mut events).await;
        assert_no_write_after_cancellation(&mut write_rx).await;
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn post_recall_ping_replacement_uses_fresh_snapshot_start_while_gated() {
        let (event_bus, engine, mut write_rx) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(40, vec![channel_info(0, -20.0, None)]),
            connected_snapshot(41, vec![channel_info(0, -40.0, None)]),
        ])
        .await;

        start_fade(
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
        )
        .await
        .unwrap();
        tokio::time::advance(Duration::from_millis(250)).await;

        start_fade(
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
        )
        .await
        .unwrap();

        event_bus.publish_lv1(7, Lv1Event::PingReceived { sequence: 42 });
        event_bus.publish_lv1(7, Lv1Event::PingReceived { sequence: 43 });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(40)).await;

        let writes = write_rx
            .recv()
            .await
            .expect("fade should write after release");
        let replacement_value = writes
            .iter()
            .find(|write| {
                write.group == 0
                    && write.channel == 0
                    && write.parameter == Lv1WriteParameter::FaderDb
            })
            .expect("replacement target should write")
            .value;
        assert!(
            replacement_value < -30.0,
            "replacement must start from the fresh LV1 value, not the paused target interpolation"
        );
    }

    #[tokio::test]
    async fn stale_fader_feedback_does_not_cancel_or_log_override_during_gate() {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let (event_bus, engine, mut write_rx) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(
                40,
                vec![channel_info(0, -20.0, None)],
            )])
            .await;

        start_fade(
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
        )
        .await
        .unwrap();

        event_bus.publish_lv1(
            6,
            Lv1Event::FaderChanged {
                group: 0,
                channel: 0,
                gain_db: -50.0,
            },
        );
        tokio::task::yield_now().await;
        assert!(
            !captured
                .events()
                .iter()
                .any(|event| event.event.as_deref() == Some("fade_manual_override"))
        );

        event_bus.publish_lv1(7, Lv1Event::PingReceived { sequence: 41 });
        event_bus.publish_lv1(7, Lv1Event::PingReceived { sequence: 42 });
        let writes = tokio::time::timeout(Duration::from_millis(300), write_rx.recv())
            .await
            .expect("stale feedback must leave the fader target active")
            .expect("fade write channel should remain open");
        assert!(
            writes
                .iter()
                .any(|write| write.parameter == Lv1WriteParameter::FaderDb)
        );
    }

    #[tokio::test]
    async fn stale_pan_feedback_does_not_cancel_target_during_gate() {
        let (event_bus, engine, mut write_rx) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(
                40,
                vec![channel_info(0, -20.0, Some(0.0))],
            )])
            .await;
        let mut events = event_bus.subscribe();

        start_fade(
            &engine,
            fade_config(
                scene(1, "Intro"),
                vec![FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::Pan,
                    target: 45.0,
                }],
                1_000,
            ),
        )
        .await
        .unwrap();

        for _ in 0..2 {
            event_bus.publish_lv1(
                6,
                Lv1Event::PanChanged {
                    group: 0,
                    channel: 0,
                    pan: -90.0,
                },
            );
        }
        tokio::task::yield_now().await;
        assert!(!matches!(
            events.try_recv(),
            Ok(AppEvent::Fade {
                event: FadeEvent::ChannelOverride { .. } | FadeEvent::ChannelCancelled { .. },
                ..
            })
        ));

        event_bus.publish_lv1(7, Lv1Event::PingReceived { sequence: 41 });
        event_bus.publish_lv1(7, Lv1Event::PingReceived { sequence: 42 });
        let writes = tokio::time::timeout(Duration::from_millis(300), write_rx.recv())
            .await
            .expect("stale feedback must leave the pan target active")
            .expect("fade write channel should remain open");
        assert!(
            writes
                .iter()
                .any(|write| write.parameter == Lv1WriteParameter::Pan)
        );
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn last_manual_override_keeps_readiness_barrier_until_timeout() {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let (event_bus, engine, mut write_rx) =
            spawn_runtime_for_ping_gate_test(vec![connected_snapshot(
                40,
                vec![channel_info(0, -20.0, None)],
            )])
            .await;
        let mut events = event_bus.subscribe();

        start_fade(
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
        )
        .await
        .unwrap();
        event_bus.publish_lv1(
            7,
            Lv1Event::FaderChanged {
                group: 0,
                channel: 0,
                gain_db: -50.0,
            },
        );

        loop {
            if matches!(
                events.recv().await.expect("event bus should remain open"),
                AppEvent::Fade {
                    event: FadeEvent::FadeCompleted,
                    ..
                }
            ) {
                break;
            }
        }
        tokio::time::advance(Duration::from_secs(5)).await;
        wait_for_fade_aborted(&mut events).await;
        assert_no_write_after_cancellation(&mut write_rx).await;
        assert!(
            captured
                .events()
                .iter()
                .any(|event| event.event.as_deref() == Some("fade_post_recall_ping_timeout"))
        );
    }

    #[tokio::test(start_paused = true, flavor = "current_thread")]
    async fn ping_at_readiness_deadline_times_out() {
        for _ in 0..8 {
            let (event_bus, engine, mut write_rx) =
                spawn_runtime_for_ping_gate_test(vec![connected_snapshot(40, vec![])]).await;
            let completed =
                start_owned_readiness(&engine, Instant::now() + Duration::from_secs(5)).await;
            event_bus.publish_lv1(7, Lv1Event::PingReceived { sequence: 41 });
            tokio::task::yield_now().await;

            tokio::time::advance(Duration::from_secs(5)).await;
            event_bus.publish_lv1(7, Lv1Event::PingReceived { sequence: 42 });
            assert_eq!(
                completed.await.unwrap(),
                Err(RecallReadinessError::TimedOut {
                    generation: 7,
                    scene_index: 2,
                    scene_name: "Verse".to_string(),
                    observed_ping_count: 1,
                }),
            );
            assert_no_write_after_cancellation(&mut write_rx).await;
        }
    }

    #[tokio::test]
    async fn post_recall_ping_barrier_logs_debug_start_and_reset() {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let (_event_bus, engine, _write_rx) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(40, vec![channel_info(0, -20.0, None)]),
            connected_snapshot(41, vec![channel_info(0, -20.0, None)]),
        ])
        .await;

        for (scene_index, scene_name) in [(1, "Intro"), (2, "Verse")] {
            start_fade(
                &engine,
                fade_config(
                    scene(scene_index, scene_name),
                    vec![FadeTarget {
                        group: 0,
                        channel: 0,
                        parameter: FadeParameter::FaderDb,
                        target: -10.0,
                    }],
                    1_000,
                ),
            )
            .await
            .unwrap();
        }

        let barrier_logs: Vec<_> = captured
            .events()
            .into_iter()
            .filter(|event| event.event.as_deref() == Some("fade_post_recall_ping_barrier"))
            .collect();
        assert_eq!(barrier_logs.len(), 2);
        for (log, expected) in barrier_logs.iter().zip([
            (
                "Fade readiness barrier started after scene recall",
                "1",
                "Intro",
                "started",
            ),
            (
                "Fade readiness barrier reset after scene recall",
                "2",
                "Verse",
                "reset",
            ),
        ]) {
            assert_eq!(log.level, Level::DEBUG);
            assert_eq!(log.message.as_deref(), Some(expected.0));
            assert_eq!(log.fields.get("generation").map(String::as_str), Some("7"));
            assert_eq!(
                log.fields.get("scene_index").map(String::as_str),
                Some(expected.1)
            );
            assert_eq!(
                log.fields.get("scene_name").map(String::as_str),
                Some(expected.2)
            );
            assert_eq!(
                log.fields.get("action").map(String::as_str),
                Some(expected.3)
            );
            for field in ["observed_ping_count", "timeout_ms", "target_count"] {
                assert!(
                    !log.fields.contains_key(field),
                    "barrier log unexpectedly included {field}"
                );
            }
        }
    }

    #[tokio::test]
    async fn repeated_scene_recall_logs_one_finishing_outcome() {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let (_event_bus, engine, _write_rx) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(
                40,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
            connected_snapshot(
                40,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
        ])
        .await;
        let repeated_scene = fade_config(
            scene(17, "Verse"),
            vec![
                FadeTarget {
                    group: 0,
                    channel: 1,
                    parameter: FadeParameter::FaderDb,
                    target: -10.0,
                },
                FadeTarget {
                    group: 0,
                    channel: 2,
                    parameter: FadeParameter::FaderDb,
                    target: -5.0,
                },
            ],
            1_000,
        );

        start_fade(&engine, repeated_scene.clone())
            .await
            .expect("initial Scene A recall should validate");
        start_fade(&engine, repeated_scene)
            .await
            .expect("repeated Scene A recall should validate");

        let logs = captured.events();
        let finishing_logs: Vec<_> = logs
            .iter()
            .filter(|event| event.event.as_deref() == Some("fade_same_scene_finishing"))
            .collect();
        assert_eq!(finishing_logs.len(), 1);
        let finishing = finishing_logs[0];
        assert_eq!(finishing.level, Level::INFO);
        assert_eq!(
            finishing.message.as_deref(),
            Some(
                "Repeated scene recall is finishing active fade targets for 17: Verse (2 targets)"
            )
        );
        assert_eq!(
            finishing.fields.get("scene_index").map(String::as_str),
            Some("17")
        );
        assert_eq!(
            finishing.fields.get("scene_name").map(String::as_str),
            Some("Verse")
        );
        assert_eq!(
            finishing.fields.get("target_count").map(String::as_str),
            Some("2")
        );
        for field in ["generation", "observed_ping_count", "timeout_ms", "action"] {
            assert!(
                !finishing.fields.contains_key(field),
                "finishing log unexpectedly included {field}"
            );
        }
        assert_eq!(
            logs.iter()
                .filter(|event| {
                    event.level == Level::INFO && event.event.as_deref() == Some("fade_started")
                })
                .count(),
            1,
            "the repeated command must not emit a second fade_started log"
        );
    }

    #[tokio::test]
    async fn repeated_scene_override_logs_one_current_value_outcome() {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let (_event_bus, engine, _write_rx) = spawn_runtime_for_ping_gate_test(vec![
            connected_snapshot(
                40,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
            connected_snapshot(
                41,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
            connected_snapshot(
                42,
                vec![channel_info(1, -20.0, None), channel_info(2, -20.0, None)],
            ),
        ])
        .await;
        let chorus = fade_config(
            scene(18, "Chorus"),
            vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: -10.0,
            }],
            1_000,
        );
        let verse = fade_config(
            scene(17, "Verse"),
            vec![FadeTarget {
                group: 0,
                channel: 2,
                parameter: FadeParameter::FaderDb,
                target: -5.0,
            }],
            1_000,
        );
        let override_verse = fade_config(
            scene(17, "Verse"),
            vec![
                FadeTarget {
                    group: 0,
                    channel: 1,
                    parameter: FadeParameter::FaderDb,
                    target: 0.0,
                },
                FadeTarget {
                    group: 0,
                    channel: 2,
                    parameter: FadeParameter::FaderDb,
                    target: 0.0,
                },
            ],
            1_000,
        );

        start_fade(&engine, chorus).await.unwrap();
        start_fade(&engine, verse).await.unwrap();
        start_fade_with_behavior(
            &engine,
            override_verse,
            SameSceneRecallBehavior::OverrideMatchingTargets,
        )
        .await
        .unwrap();

        let logs = captured.events();
        let overriding_logs: Vec<_> = logs
            .iter()
            .filter(|event| event.event.as_deref() == Some("fade_same_scene_overriding"))
            .collect();
        assert_eq!(overriding_logs.len(), 1);
        let overriding = overriding_logs[0];
        assert_eq!(overriding.level, Level::INFO);
        assert_eq!(
            overriding.message.as_deref(),
            Some(
                "Repeated scene recall is overriding active fade targets from their current values for 17: Verse (2 targets)"
            )
        );
        assert_eq!(
            overriding.fields.get("scene_index").map(String::as_str),
            Some("17")
        );
        assert_eq!(
            overriding.fields.get("scene_name").map(String::as_str),
            Some("Verse")
        );
        assert_eq!(
            overriding.fields.get("target_count").map(String::as_str),
            Some("2")
        );
        for field in ["generation", "observed_ping_count", "timeout_ms", "action"] {
            assert!(
                !overriding.fields.contains_key(field),
                "override log unexpectedly included {field}"
            );
        }
        assert_eq!(
            logs.iter()
                .filter(|event| {
                    event.level == Level::INFO && event.event.as_deref() == Some("fade_started")
                })
                .count(),
            2,
            "the override command must not emit a third fade_started log"
        );
    }
}

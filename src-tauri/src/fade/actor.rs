//! Fade engine actor — animates LV1 faders over time.

use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::fade::commands::FadeCommand;
use crate::fade::events::FadeEvent;
use crate::fade::handle::FadeEngineHandle;
use crate::fade::state::EngineState;
use crate::fade::tick::{ActiveTarget, ActiveTargetInit, TICK_HZ};
use crate::fade::types::{FadeParameter, FadeTarget};
use crate::lv1::commands::{Lv1ParameterWrite, Lv1WriteParameter};
use crate::runtime::commands::AppCommandBus;
use crate::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};

pub fn spawn_engine(command_bus: AppCommandBus, event_bus: AppEventBus) -> FadeEngineHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    tokio::spawn(run_engine(command_bus, event_bus, cmd_rx));
    FadeEngineHandle::new(cmd_tx)
}

async fn run_engine(
    command_bus: AppCommandBus,
    event_bus: AppEventBus,
    mut cmd_rx: mpsc::Receiver<FadeCommand>,
) {
    let mut app_events = event_bus.subscribe();
    let mut state = EngineState::new(event_bus.clone());
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

        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    None => break,
                Some(FadeCommand::RecallSceneFade { config, expected_generation, reply }) => {
                        let scene_index = config.scene.index;
                        let scene_name = config.scene.name.clone();
                        let duration_ms = config.duration_ms;
                        let target_count = config.targets.len();
                        let result = handle_recall_scene_fade(&command_bus, &mut state, config, expected_generation).await;

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
                                let _ = reply.send(Ok(()));
                            }
                            Err(err) => {
                                let _ = reply.send(Err(err));
                            }
                        }
                    }
                    Some(FadeCommand::AbortAll { reply }) => {
                        state.cancel_all_in_place();
                        tick_interval = None;
                        state.fan_out(FadeEvent::FadeAborted);
                        let _ = reply.send(Ok(()));
                    }
                }
            }

            _ = tick_fut => {
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
                                send_batch_if_generation(
                                    &command_bus,
                                    &state.event_bus,
                                    expected_generation,
                                    writes,
                                )
                                .await
                            }
                            None => {
                                send_batch(&command_bus, &state.event_bus, writes).await;
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
                    Ok(AppEvent::Lv1(crate::lv1::events::Lv1Event::FaderChanged { group, channel, gain_db })) => {
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
                    Ok(AppEvent::Lv1(crate::lv1::events::Lv1Event::PanChanged { group, channel, pan })) => {
                        handle_pan_family_pan_report(
                            &mut state,
                            group,
                            channel,
                            pan,
                            &mut tick_interval,
                            &mut fade_completed_emitted,
                        );
                    }
                    Ok(AppEvent::Lv1(crate::lv1::events::Lv1Event::Disconnected { .. })) => {
                        if state.is_active() {
                            state.cancel_all_in_place();
                            tick_interval = None;
                            fade_completed_emitted = false;
                            tracing::warn!(event = "fade_aborted", "Fade aborted");
                            state.fan_out(FadeEvent::FadeAborted);
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        log_lagged_subscriber("fade-engine", count);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
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
    command_bus: &AppCommandBus,
    state: &mut EngineState,
    config: crate::fade::types::FadeConfig,
    expected_generation: Option<u64>,
) -> Result<(), crate::runtime::commands::AppCommandError> {
    if let Some(expected_generation) = expected_generation
        && command_bus.get_generation().await != expected_generation
    {
        return Err(crate::runtime::commands::AppCommandError::StaleGeneration);
    }

    if config.targets.is_empty() {
        return Ok(());
    }

    let snapshot = command_bus.get_lv1_state().await?;
    if let Some(expected_generation) = expected_generation
        && command_bus.get_generation().await != expected_generation
    {
        return Err(crate::runtime::commands::AppCommandError::StaleGeneration);
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
            if !send_batch_if_generation(command_bus, &state.event_bus, expected_generation, writes)
                .await
            {
                return Err(crate::runtime::commands::AppCommandError::StaleGeneration);
            }
        } else {
            send_batch(command_bus, &state.event_bus, writes).await;
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

    Ok(())
}

fn live_value_for_snapshot(
    channel: &crate::lv1::types::ChannelInfo,
    target: &FadeTarget,
) -> Option<f64> {
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

async fn send_batch(
    command_bus: &AppCommandBus,
    event_bus: &AppEventBus,
    writes: Vec<Lv1ParameterWrite>,
) {
    if let Err(err) = command_bus.write_batch(writes).await {
        let reason = format!("{err:?}");
        tracing::error!(event = "fade_write_failed", reason = %reason, "Fade write failed: {reason}");
        event_bus.publish(AppEvent::Fade(FadeEvent::WriteFailed { reason }));
    }
}

async fn send_batch_if_generation(
    command_bus: &AppCommandBus,
    event_bus: &AppEventBus,
    expected_generation: u64,
    writes: Vec<Lv1ParameterWrite>,
) -> bool {
    if let Err(err) = command_bus
        .write_batch_if_generation(expected_generation, writes)
        .await
    {
        let reason = format!("{err:?}");
        tracing::error!(event = "fade_write_failed", reason = %reason, "Fade write failed: {reason}");
        event_bus.publish(AppEvent::Fade(FadeEvent::WriteFailed { reason }));
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
    use crate::lv1::commands::{Lv1Command, Lv1ParameterWrite, Lv1WriteParameter};
    use crate::runtime::events::AppEventBus;

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
        let lv1 = crate::lv1::handle::Lv1ActorHandle::new(tx);
        let bus = AppCommandBus::new();
        bus.set_lv1(Some(lv1)).await;
        let engine = spawn_engine(bus.clone(), event_bus.clone());
        bus.set_fade(Some(engine.clone())).await;
        (event_bus, engine, rx)
    }

    async fn assert_pan_family_aux_event_does_not_override(parameter: FadeParameter) {
        let (event_bus, engine, mut rx) = spawn_runtime_for_test().await;
        let mut events = event_bus.subscribe();
        let (write_tx, mut write_rx) = tokio::sync::mpsc::channel::<Vec<Lv1ParameterWrite>>(8);

        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                match command {
                    Lv1Command::GetState { reply } => {
                        let _ = reply.send(crate::lv1::types::Lv1StateSnapshot {
                            connection: crate::lv1::types::ConnectionStatus::Connected,
                            scene: None,
                            scene_list: vec![],
                            channels: vec![],
                        });
                    }
                    Lv1Command::WriteBatch(writes) => {
                        let _ = write_tx.send(writes).await;
                    }
                    _ => {}
                }
            }
        });

        engine
            .start_fade(fade_config(
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
            ))
            .await
            .unwrap();

        // Drain the initial fade setup writes and any immediate events before the aux report.
        while write_rx.try_recv().is_ok() {}
        while events.try_recv().is_ok() {}

        event_bus.publish(AppEvent::Lv1(match parameter {
            FadeParameter::Balance => crate::lv1::events::Lv1Event::BalanceChanged {
                group: 0,
                channel: 0,
                balance: -45.0,
            },
            FadeParameter::Width => crate::lv1::events::Lv1Event::WidthChanged {
                group: 0,
                channel: 0,
                width: -45.0,
            },
            _ => unreachable!(),
        }));

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
                AppEvent::Fade(FadeEvent::ChannelOverride { .. }) => saw_override = true,
                AppEvent::Fade(FadeEvent::ChannelCancelled { .. }) => saw_cancelled = true,
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
        let mut state = EngineState::new(event_bus);
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
                AppEvent::Fade(FadeEvent::ChannelOverride {
                    group,
                    channel,
                    parameter,
                }) => {
                    assert_eq!((group, channel, parameter), (0, 0, FadeParameter::Pan));
                    saw_override = true;
                }
                AppEvent::Fade(FadeEvent::ChannelCancelled {
                    group,
                    channel,
                    parameter,
                }) => {
                    cancelled.insert((group, channel, parameter));
                }
                AppEvent::Fade(FadeEvent::FadeCompleted) => {
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
        let mut state = EngineState::new(event_bus);
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
                AppEvent::Fade(FadeEvent::ChannelOverride {
                    group,
                    channel,
                    parameter,
                }) => {
                    assert_eq!((group, channel, parameter), (0, 0, FadeParameter::Pan));
                    saw_override = true;
                }
                AppEvent::Fade(FadeEvent::ChannelCancelled {
                    group,
                    channel,
                    parameter,
                }) => {
                    cancelled.insert((group, channel, parameter));
                }
                AppEvent::Fade(FadeEvent::FadeCompleted) => saw_fade_completed = true,
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
        let mut state = EngineState::new(event_bus);
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
                AppEvent::Fade(FadeEvent::ChannelOverride { .. }) => {
                    panic!("unexpected ChannelOverride event")
                }
                AppEvent::Fade(FadeEvent::ChannelCancelled { .. }) => {
                    panic!("unexpected ChannelCancelled event")
                }
                AppEvent::Fade(FadeEvent::FadeCompleted) => {
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
        let mut state = EngineState::new(event_bus);
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
                AppEvent::Fade(FadeEvent::ChannelOverride { .. }) => {
                    panic!("unexpected ChannelOverride event")
                }
                AppEvent::Fade(FadeEvent::ChannelCancelled { .. }) => {
                    panic!("unexpected ChannelCancelled event")
                }
                AppEvent::Fade(FadeEvent::FadeCompleted) => {
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
        let mut state = EngineState::new(event_bus);
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
                AppEvent::Fade(FadeEvent::ChannelOverride {
                    group,
                    channel,
                    parameter,
                }) => {
                    assert_eq!((group, channel, parameter), (0, 0, FadeParameter::Pan));
                    saw_override = true;
                }
                AppEvent::Fade(FadeEvent::ChannelCancelled {
                    group,
                    channel,
                    parameter,
                }) => {
                    cancelled.insert((group, channel, parameter));
                }
                AppEvent::Fade(FadeEvent::FadeCompleted) => saw_fade_completed = true,
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
                        let _ = reply.send(crate::lv1::types::Lv1StateSnapshot {
                            connection: crate::lv1::types::ConnectionStatus::Connected,
                            scene: None,
                            scene_list: vec![],
                            channels: vec![],
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

        engine
            .start_fade(fade_config(
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
            ))
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
        let lv1 = crate::lv1::handle::Lv1ActorHandle::new(tx);
        let bus = AppCommandBus::new();
        bus.set_lv1(Some(lv1)).await;
        let engine = spawn_engine(bus.clone(), event_bus.clone());
        bus.set_fade(Some(engine.clone())).await;

        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let mut result_tx = Some(result_tx);
            while let Some(command) = rx.recv().await {
                match command {
                    Lv1Command::GetState { reply } => {
                        let _ = reply.send(crate::lv1::types::Lv1StateSnapshot {
                            connection: crate::lv1::types::ConnectionStatus::Connected,
                            scene: None,
                            scene_list: vec![],
                            channels: vec![],
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

        engine
            .start_fade(fade_config(
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
            ))
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
        let bus = AppCommandBus::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        bus.set_lv1(Some(crate::lv1::handle::Lv1ActorHandle::new(lv1_tx)))
            .await;
        bus.set_generation(3).await;

        let mut state = EngineState::new(event_bus);
        let result = handle_recall_scene_fade(
            &bus,
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

        assert_eq!(
            result,
            Err(crate::runtime::commands::AppCommandError::StaleGeneration)
        );
        assert!(lv1_rx.try_recv().is_err());
        assert!(state.channels.is_empty());
    }

    #[tokio::test]
    async fn generation_flip_while_lv1_snapshot_is_pending_is_rejected_after_snapshot() {
        let event_bus = AppEventBus::default();
        let bus = AppCommandBus::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        bus.set_lv1(Some(crate::lv1::handle::Lv1ActorHandle::new(lv1_tx)))
            .await;
        bus.set_generation(3).await;

        let bus_for_lv1 = bus.clone();
        tokio::spawn(async move {
            if let Some(crate::lv1::commands::Lv1Command::GetState { reply }) = lv1_rx.recv().await
            {
                bus_for_lv1.set_generation(4).await;
                let _ = reply.send(crate::lv1::types::Lv1StateSnapshot {
                    connection: crate::lv1::types::ConnectionStatus::Connected,
                    scene: None,
                    scene_list: vec![],
                    channels: vec![],
                });
            }
        });

        let mut state = EngineState::new(event_bus);
        let result = handle_recall_scene_fade(
            &bus,
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

        assert_eq!(
            result,
            Err(crate::runtime::commands::AppCommandError::StaleGeneration)
        );
        assert!(state.channels.is_empty());
    }

    #[tokio::test]
    async fn zero_duration_recall_fade_uses_generation_checked_write_batch() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let bus = AppCommandBus::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        bus.set_lv1(Some(crate::lv1::handle::Lv1ActorHandle::new(lv1_tx)))
            .await;
        bus.set_generation(4).await;

        let (write_tx, write_rx) = tokio::sync::oneshot::channel::<()>();
        let write_tx = std::sync::Arc::new(std::sync::Mutex::new(Some(write_tx)));
        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                if let crate::lv1::commands::Lv1Command::WriteBatch(_) = command
                    && let Some(tx) = write_tx.lock().unwrap().take()
                {
                    let _ = tx.send(());
                    break;
                }
            }
        });

        let mut state = EngineState::new(event_bus);
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
            &bus,
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

        let event = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
            .await
            .unwrap()
            .unwrap();
        match event {
            AppEvent::Fade(FadeEvent::WriteFailed { reason }) => {
                assert!(
                    reason
                        == crate::runtime::commands::AppCommandError::StaleGeneration.to_string()
                        || reason == "StaleGeneration"
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn complete_fade_is_idempotent() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let mut state = EngineState::new(event_bus.clone());
        let mut tick_interval = None;
        let mut emitted = false;

        complete_fade(&mut tick_interval, &mut state, &mut emitted);
        complete_fade(&mut tick_interval, &mut state, &mut emitted);
        let event = tokio::time::timeout(std::time::Duration::from_millis(100), events.recv())
            .await
            .unwrap()
            .unwrap();

        assert!(matches!(event, AppEvent::Fade(FadeEvent::FadeCompleted)));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), events.recv())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn timed_recall_fade_tick_uses_generation_checked_write_batch() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let bus = AppCommandBus::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        bus.set_lv1(Some(crate::lv1::handle::Lv1ActorHandle::new(lv1_tx)))
            .await;
        bus.set_generation(3).await;

        let (write_tx, write_rx) = tokio::sync::oneshot::channel::<()>();
        let write_tx = std::sync::Arc::new(std::sync::Mutex::new(Some(write_tx)));
        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                if let crate::lv1::commands::Lv1Command::WriteBatch(_) = command
                    && let Some(tx) = write_tx.lock().unwrap().take()
                {
                    let _ = tx.send(());
                    break;
                }
            }
        });

        let mut state = EngineState::new(event_bus);
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

        bus.set_generation(4).await;
        let writes = vec![build_parameter_write(0, 0, FadeParameter::FaderDb, -12.5)];
        let sent = send_batch_if_generation(&bus, &state.event_bus, 3, writes).await;
        assert!(!sent);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), write_rx)
                .await
                .is_err()
        );

        match events.recv().await.unwrap() {
            AppEvent::Fade(FadeEvent::WriteFailed { reason }) => {
                assert!(
                    reason
                        == crate::runtime::commands::AppCommandError::StaleGeneration.to_string()
                        || reason == "StaleGeneration"
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn mixed_generation_writes_on_same_tick_route_separately() {
        let _event_bus = AppEventBus::default();
        let bus = AppCommandBus::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(4);
        bus.set_lv1(Some(crate::lv1::handle::Lv1ActorHandle::new(lv1_tx)))
            .await;

        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    crate::lv1::commands::Lv1Command::WriteBatch(writes) => {
                        assert_eq!(writes.len(), 1);
                    }
                    _ => panic!("unexpected command"),
                }
            }
        });

        let mut state = EngineState::new(AppEventBus::default());
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
        let mut events = event_bus.subscribe();
        let bus = AppCommandBus::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        bus.set_lv1(Some(crate::lv1::handle::Lv1ActorHandle::new(lv1_tx)))
            .await;

        let mut state = EngineState::new(event_bus);
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

        bus.set_generation(4).await;
        let writes = vec![build_parameter_write(0, 0, FadeParameter::FaderDb, -12.5)];
        let sent = send_batch_if_generation(&bus, &state.event_bus, 3, writes).await;
        assert!(!sent);
        cancel_generation_owned_targets(&mut state, 3);

        assert!(state.channels.is_empty());
        assert!(lv1_rx.try_recv().is_err());
        let mut saw_write_failed = false;
        loop {
            match tokio::time::timeout(std::time::Duration::from_millis(50), events.recv()).await {
                Ok(Ok(AppEvent::Fade(FadeEvent::WriteFailed { reason }))) => {
                    assert!(
                        reason
                            == crate::runtime::commands::AppCommandError::StaleGeneration
                                .to_string()
                            || reason == "StaleGeneration"
                    );
                    saw_write_failed = true;
                }
                Ok(Ok(AppEvent::Fade(FadeEvent::ChannelCancelled { .. }))) => {}
                Ok(Ok(other)) => panic!("unexpected event: {other:?}"),
                Ok(Err(_)) | Err(_) => break,
            }
        }
        assert!(saw_write_failed);
    }
}

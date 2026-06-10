//! Fade engine actor — animates LV1 faders over time.

use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::fade::commands::FadeCommand;
use crate::fade::events::FadeEvent;
use crate::fade::handle::FadeEngineHandle;
use crate::fade::state::EngineState;
use crate::fade::tick::{ActiveTarget, ActiveTargetInit, TICK_HZ};
use crate::fade::types::{FadeParameter, FadeTarget};
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
    let mut state = EngineState::new(event_bus);
    let mut tick_interval: Option<tokio::time::Interval> = None;

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
                Some(FadeCommand::RecallSceneFade { config, reply }) => {
                        if config.targets.is_empty() {
                            let _ = reply.send(Ok(()));
                            continue;
                        }

                        let snapshot = match command_bus.get_lv1_state().await {
                            Ok(snapshot) => snapshot,
                            Err(err) => {
                                let _ = reply.send(Err(err));
                                continue;
                            }
                        };
                        let now = Instant::now();
                        let duration = Duration::from_millis(config.duration_ms);

                        if duration.is_zero() {
                            for target in &config.targets {
                                send_target(&command_bus, target.group, target.channel, target.parameter, target.target).await;
                                state.channels.retain(|ch| ch.key != target.key());
                                state.fan_out(FadeEvent::ChannelCompleted {
                                    group: target.group,
                                    channel: target.channel,
                                });
                            }
                            if !state.is_active() {
                                tick_interval = None;
                                state.fan_out(FadeEvent::FadeCompleted);
                            }
                            let _ = reply.send(Ok(()));
                            continue;
                        }

                        for target in &config.targets {
                            let start_value = state
                                .channels
                                .iter()
                                .find(|ch| ch.key == target.key())
                                .map(|ch| if ch.is_done(now) { ch.target_value } else { ch.value_at(now) })
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
                                scene: config.scene.clone(),
                                key: target.key(),
                                group: target.group,
                                channel: target.channel,
                                start_value,
                                target_value: target.target,
                                curve: config.curve,
                                duration,
                                started_at: now,
                            }));
                        }

                        let mut interval = tokio::time::interval(Duration::from_millis(1000 / TICK_HZ));
                        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                        tick_interval = Some(interval);

                        state.fan_out(FadeEvent::FadeStarted);
                        let _ = reply.send(Ok(()));
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

                for (i, ch) in state.channels.iter_mut().enumerate() {
                    if ch.is_done(now) {
                        let target_db = ch.exact_final_send();
                        send_target(&command_bus, ch.group, ch.channel, ch.key.parameter, target_db).await;
                        completed_events.push(FadeEvent::ChannelCompleted { group: ch.group, channel: ch.channel });
                        done_indices.push(i);
                        continue;
                    }

                    if let Some(new_value) = ch.next_send(now) {
                        send_target(&command_bus, ch.group, ch.channel, ch.key.parameter, new_value).await;
                    }
                }

                for i in done_indices.into_iter().rev() {
                    state.channels.remove(i);
                }

                for event in completed_events {
                    state.fan_out(event);
                }

                if !state.is_active() {
                    tick_interval = None;
                    state.fan_out(FadeEvent::FadeCompleted);
                }
            }

            app_event = app_events.recv() => {
                match app_event {
                    Ok(AppEvent::Lv1(crate::lv1::events::Lv1Event::FaderChanged { group, channel, gain_db })) => {
                        if let Some(pos) = state.channels.iter().position(|ch| ch.group == group && ch.channel == channel && ch.key.parameter == FadeParameter::FaderDb)
                            && state.channels[pos].is_override(gain_db)
                        {
                            state.fan_out(FadeEvent::ChannelOverride { group, channel });
                            state.channels.remove(pos);
                            state.fan_out(FadeEvent::ChannelCancelled { group, channel });

                            if !state.is_active() {
                                tick_interval = None;
                            }
                        }
                    }
                    Ok(AppEvent::Lv1(crate::lv1::events::Lv1Event::PanChanged { group, channel, pan })) => {
                        cancel_pan_family_overrides(&mut state, group, channel, pan, &mut tick_interval);
                    }
                    Ok(AppEvent::Lv1(crate::lv1::events::Lv1Event::BalanceChanged { group, channel, balance })) => {
                        cancel_pan_family_overrides(&mut state, group, channel, balance, &mut tick_interval);
                    }
                    Ok(AppEvent::Lv1(crate::lv1::events::Lv1Event::WidthChanged { group, channel, width })) => {
                        cancel_pan_family_overrides(&mut state, group, channel, width, &mut tick_interval);
                    }
                    Ok(AppEvent::Lv1(crate::lv1::events::Lv1Event::Disconnected)) => {
                        if state.is_active() {
                            state.cancel_all_in_place();
                            tick_interval = None;
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

async fn send_target(
    command_bus: &AppCommandBus,
    group: i32,
    channel: i32,
    parameter: FadeParameter,
    value: f64,
) {
    let _ = match parameter {
        FadeParameter::FaderDb => command_bus.set_gain(group, channel, value).await,
        FadeParameter::Pan => command_bus.set_pan(group, channel, value).await,
        FadeParameter::Balance => command_bus.set_balance(group, channel, value).await,
        FadeParameter::Width => command_bus.set_width(group, channel, value).await,
    };
}

fn cancel_pan_family_overrides(
    state: &mut EngineState,
    group: i32,
    channel: i32,
    reported_value: f64,
    tick_interval: &mut Option<tokio::time::Interval>,
) {
    let mut cancel = false;
    for ch in state
        .channels
        .iter()
        .filter(|ch| ch.group == group && ch.channel == channel && ch.key.parameter.is_pan_family())
    {
        if ch.is_override(reported_value) {
            cancel = true;
            break;
        }
    }

    if !cancel {
        return;
    }

    state.channels.retain(|ch| {
        !(ch.group == group && ch.channel == channel && ch.key.parameter.is_pan_family())
    });
    state.fan_out(FadeEvent::ChannelOverride { group, channel });
    state.fan_out(FadeEvent::ChannelCancelled { group, channel });

    if !state.is_active() {
        *tick_interval = None;
    }
}

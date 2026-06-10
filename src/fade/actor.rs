//! Fade engine actor — animates LV1 faders over time.

use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::fade::commands::FadeCommand;
use crate::fade::events::FadeEvent;
use crate::fade::handle::FadeEngineHandle;
use crate::fade::state::{EngineState, finish_scene_channels};
use crate::fade::tick::{ActiveTarget, ActiveTargetInit, TICK_HZ};
use crate::fade::types::FadeParameter;
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
                        if state.has_active_scene(&config.scene) {
                            finish_scene_channels(&mut state, &command_bus, &config.scene).await;
                            if !state.is_active() {
                                tick_interval = None;
                                state.fan_out(FadeEvent::FadeCompleted);
                            }
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
                                if target.parameter == FadeParameter::FaderDb {
                                    let _ = command_bus
                                        .set_gain(target.group, target.channel, target.target)
                                        .await;
                                }
                                state.channels.retain(|ch| ch.key != target.key());
                                if target.parameter == FadeParameter::FaderDb {
                                    state.fan_out(FadeEvent::ChannelCompleted {
                                        group: target.group,
                                        channel: target.channel,
                                    });
                                }
                            }
                            if !state.is_active() {
                                tick_interval = None;
                                state.fan_out(FadeEvent::FadeCompleted);
                            }
                            let _ = reply.send(Ok(()));
                            continue;
                        }

                        for target in &config.targets {
                            let start_db = state
                                .channels
                                .iter()
                                .find(|ch| ch.key == target.key())
                                .map(|ch| if ch.is_done(now) { ch.target_db } else { ch.value_at(now) })
                                .or_else(|| {
                                    snapshot
                                        .channels
                                        .iter()
                                        .find(|ch| ch.group == target.group && ch.channel == target.channel)
                                        .map(|ch| ch.gain_db)
                                })
                                .unwrap_or(target.target);

                            state.channels.retain(|ch| ch.key != target.key());
                            state.channels.push(ActiveTarget::new(ActiveTargetInit {
                                scene: config.scene.clone(),
                                key: target.key(),
                                group: target.group,
                                channel: target.channel,
                                start_db,
                                target_db: target.target,
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
                        let _ = command_bus.set_gain(ch.group, ch.channel, target_db).await;
                        completed_events.push(FadeEvent::ChannelCompleted { group: ch.group, channel: ch.channel });
                        done_indices.push(i);
                        continue;
                    }

                    if let Some(new_db) = ch.next_send(now) {
                        let _ = command_bus.set_gain(ch.group, ch.channel, new_db).await;
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
                        if let Some(pos) = state.channels.iter().position(|ch| ch.group == group && ch.channel == channel)
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

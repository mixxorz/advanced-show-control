//! Fade engine actor — animates LV1 faders over time.

use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

use crate::fade::tick::{ActiveChannel, TICK_HZ};
use crate::fade::types::{FadeCommand, FadeConfig, FadeEvent};
use crate::runtime::commands::{AppCommandBus, AppCommandError};
use crate::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};

// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FadeEngineHandle {
    tx: mpsc::Sender<FadeCommand>,
}

impl FadeEngineHandle {
    pub fn new(tx: mpsc::Sender<FadeCommand>) -> Self {
        Self { tx }
    }

    pub async fn start_fade(&self, config: FadeConfig) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(FadeCommand::RecallSceneFade { config, reply })
            .await
            .map_err(|_| AppCommandError::FadeUnavailable)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

    pub async fn abort_all(&self) -> Result<(), AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(FadeCommand::AbortAll { reply })
            .await
            .map_err(|_| AppCommandError::FadeUnavailable)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
    }

}

pub fn spawn_engine(command_bus: AppCommandBus, event_bus: AppEventBus) -> FadeEngineHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    tokio::spawn(run_engine(command_bus, event_bus, cmd_rx));
    FadeEngineHandle::new(cmd_tx)
}

// ---------------------------------------------------------------------------
// Actor internals
// ---------------------------------------------------------------------------

struct EngineState {
    channels: Vec<ActiveChannel>,
    event_bus: AppEventBus,
}

impl EngineState {
    fn new(event_bus: AppEventBus) -> Self {
        Self {
            channels: Vec::new(),
            event_bus,
        }
    }

    fn fan_out(&mut self, event: FadeEvent) {
        self.event_bus.publish(AppEvent::Fade(event));
    }

    fn is_active(&self) -> bool {
        !self.channels.is_empty()
    }

    fn cancel_all_in_place(&mut self) {
        self.channels.clear();
    }
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
        // Build the tick future: only poll when active
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
                        let snapshot = match command_bus.get_lv1_state().await {
                            Ok(snapshot) => snapshot,
                            Err(err) => {
                                let _ = reply.send(Err(err));
                                continue;
                            }
                        };
                        state.cancel_all_in_place();
                        let now = Instant::now();
                        let duration = Duration::from_millis(config.duration_ms);

                        for target in &config.targets {
                            let start_db = snapshot.channels.iter()
                                .find(|ch| ch.group == target.group && ch.channel == target.channel)
                                .map(|ch| ch.gain_db)
                                .unwrap_or(target.target_db);

                            state.channels.push(ActiveChannel::new(
                                config.scene.clone(),
                                target.group,
                                target.channel,
                                start_db,
                                target.target_db,
                                config.curve,
                                duration,
                                now,
                            ));
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

                for (i, ch) in state.channels.iter_mut().enumerate() {
                    if let Some(new_db) = ch.next_send(now) {
                        let _ = command_bus.set_gain(ch.group, ch.channel, new_db).await;
                    }
                    if ch.is_done(now) {
                        done_indices.push(i);
                    }
                }

                // Remove completed channels (reverse order to preserve indices)
                for i in done_indices.into_iter().rev() {
                    state.channels.remove(i);
                }

                if !state.is_active() {
                    tick_interval = None;
                    state.fan_out(FadeEvent::FadeCompleted);
                }
            }

            app_event = app_events.recv() => {
                match app_event {
                    Ok(AppEvent::Lv1(crate::lv1::messages::Lv1Event::FaderChanged { group, channel, gain_db })) => {
                        if let Some(pos) = state.channels.iter().position(|ch| ch.group == group && ch.channel == channel) {
                            if state.channels[pos].is_override(gain_db) {
                                state.fan_out(FadeEvent::ChannelOverride { group, channel });
                                state.channels.remove(pos);
                                state.fan_out(FadeEvent::ChannelCancelled { group, channel });

                                if !state.is_active() {
                                    tick_interval = None;
                                }
                            }
                        }
                    }
                    Ok(AppEvent::Lv1(crate::lv1::messages::Lv1Event::Disconnected)) => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::curve::FadeCurve;
    use crate::fade::types::{FadeCommand, FadeSceneIdentity, FadeTarget};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn closed_command_channel_returns_fade_unavailable() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let handle = FadeEngineHandle { tx };

        let result = handle
            .start_fade(FadeConfig {
                scene: FadeSceneIdentity {
                    index: 1,
                    name: "Intro".to_string(),
                },
                targets: vec![FadeTarget {
                    group: 0,
                    channel: 1,
                    target_db: -12.0,
                }],
                duration_ms: 100,
                curve: FadeCurve::Linear,
            })
            .await;

        assert_eq!(result, Err(AppCommandError::FadeUnavailable));
    }

    #[tokio::test]
    async fn dropped_reply_channel_returns_reply_channel_closed() {
        let (tx, mut rx) = mpsc::channel(1);
        let handle = FadeEngineHandle { tx };

        let task = tokio::spawn(async move {
            handle
                .start_fade(FadeConfig {
                    scene: FadeSceneIdentity {
                        index: 1,
                        name: "Intro".to_string(),
                    },
                    targets: vec![FadeTarget {
                        group: 0,
                        channel: 1,
                        target_db: -12.0,
                    }],
                    duration_ms: 100,
                    curve: FadeCurve::Linear,
                })
                .await
        });

        match rx.recv().await.unwrap() {
            FadeCommand::RecallSceneFade { reply, .. } => drop(reply),
            _ => panic!("unexpected command"),
        }

        assert_eq!(
            task.await.unwrap(),
            Err(AppCommandError::ReplyChannelClosed)
        );
    }
}

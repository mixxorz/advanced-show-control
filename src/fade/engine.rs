//! Fade engine actor — animates LV1 faders over time.

use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::fade::tick::{ActiveChannel, TICK_HZ};
use crate::fade::types::{FadeCommand, FadeConfig, FadeEvent};
use crate::lv1::messages::Lv1Event;
use crate::lv1::state::Lv1ActorHandle;
use crate::runtime::commands::AppCommandError;

// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FadeEngineHandle {
    tx: mpsc::Sender<FadeCommand>,
}

impl FadeEngineHandle {
    pub async fn start_fade(&self, config: FadeConfig) -> Result<(), AppCommandError> {
        self.tx
            .send(FadeCommand::StartFade { config })
            .await
            .map_err(|_| AppCommandError::FadeUnavailable)
    }

    pub async fn abort_all(&self) -> Result<(), AppCommandError> {
        self.tx
            .send(FadeCommand::AbortAll)
            .await
            .map_err(|_| AppCommandError::FadeUnavailable)
    }

    pub async fn finish_now(&self) -> Result<(), AppCommandError> {
        self.tx
            .send(FadeCommand::FinishNow)
            .await
            .map_err(|_| AppCommandError::FadeUnavailable)
    }

    pub async fn subscribe(&self) -> mpsc::Receiver<FadeEvent> {
        let (tx, rx) = mpsc::channel(64);
        let _ = self.tx.send(FadeCommand::Subscribe { tx }).await;
        rx
    }
}

pub fn spawn_engine(lv1: Lv1ActorHandle) -> FadeEngineHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel(32);
    tokio::spawn(run_engine(lv1, cmd_rx));
    FadeEngineHandle { tx: cmd_tx }
}

// ---------------------------------------------------------------------------
// Actor internals
// ---------------------------------------------------------------------------

struct EngineState {
    channels: Vec<ActiveChannel>,
    subscribers: Vec<mpsc::Sender<FadeEvent>>,
}

impl EngineState {
    fn new() -> Self {
        Self {
            channels: Vec::new(),
            subscribers: Vec::new(),
        }
    }

    fn fan_out(&mut self, event: FadeEvent) {
        self.subscribers
            .retain(|tx| tx.try_send(event.clone()).is_ok());
    }

    fn is_active(&self) -> bool {
        !self.channels.is_empty()
    }

    fn cancel_all_in_place(&mut self) {
        self.channels.clear();
    }
}

async fn run_engine(lv1: Lv1ActorHandle, mut cmd_rx: mpsc::Receiver<FadeCommand>) {
    let mut lv1_events = lv1.subscribe().await;
    let mut state = EngineState::new();
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
                    Some(FadeCommand::Subscribe { tx }) => {
                        state.subscribers.push(tx);
                    }
                    Some(FadeCommand::StartFade { config }) => {
                        state.cancel_all_in_place();

                        let snapshot = lv1.get_state().await;
                        let now = Instant::now();
                        let duration = Duration::from_millis(config.duration_ms);

                        for target in &config.targets {
                            let start_db = snapshot.channels.iter()
                                .find(|ch| ch.group == target.group && ch.channel == target.channel)
                                .map(|ch| ch.gain_db)
                                .unwrap_or(target.target_db);

                            state.channels.push(ActiveChannel::new(
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
                    }
                    Some(FadeCommand::AbortAll) => {
                        state.cancel_all_in_place();
                        tick_interval = None;
                        state.fan_out(FadeEvent::FadeAborted);
                    }
                    Some(FadeCommand::FinishNow) => {
                        for ch in &state.channels {
                            let _ = lv1.set_gain(ch.group, ch.channel, ch.target_db).await;
                        }
                        state.cancel_all_in_place();
                        tick_interval = None;
                        state.fan_out(FadeEvent::FadeCompleted);
                    }
                }
            }

            _ = tick_fut => {
                let now = Instant::now();
                let mut done_indices = Vec::new();

                for (i, ch) in state.channels.iter_mut().enumerate() {
                    if let Some(new_db) = ch.next_send(now) {
                        let _ = lv1.set_gain(ch.group, ch.channel, new_db).await;
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

            lv1_event = lv1_events.recv() => {
                match lv1_event {
                    Some(Lv1Event::FaderChanged { group, channel, gain_db }) => {
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
                    Some(Lv1Event::Disconnected) => {
                        if state.is_active() {
                            state.cancel_all_in_place();
                            tick_interval = None;
                            state.fan_out(FadeEvent::FadeAborted);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::curve::FadeCurve;
    use crate::fade::types::FadeTarget;

    #[tokio::test]
    async fn closed_command_channel_returns_fade_unavailable() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let handle = FadeEngineHandle { tx };

        let result = handle
            .start_fade(FadeConfig {
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
}

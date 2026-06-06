//! Fade engine actor — animates LV1 faders over time.

use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::fade::curve::{FadeCurve, interpolate};
use crate::lv1::state::Lv1ActorHandle;

pub const TICK_HZ: u64 = 25;
pub const MIN_SEND_DELTA_DB: f64 = 0.1;
pub const OVERRIDE_THRESHOLD_DB: f64 = 0.5;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FadeTarget {
    pub group: i32,
    pub channel: i32,
    pub target_db: f64,
}

#[derive(Debug, Clone)]
pub struct FadeConfig {
    pub targets: Vec<FadeTarget>,
    pub duration_ms: u64,
    pub curve: FadeCurve,
}

pub enum FadeCommand {
    StartFade { config: FadeConfig },
    AbortAll,
    FinishNow,
    Subscribe { tx: mpsc::Sender<FadeEvent> },
}

#[derive(Debug, Clone)]
pub enum FadeEvent {
    FadeStarted,
    FadeCompleted,
    FadeAborted,
    ChannelOverride { group: i32, channel: i32 },
    ChannelCancelled { group: i32, channel: i32 },
}

// ---------------------------------------------------------------------------
// Internal tick logic (pure — no async, easy to test)
// ---------------------------------------------------------------------------

pub(crate) struct ActiveChannel {
    pub group: i32,
    pub channel: i32,
    pub start_db: f64,
    pub target_db: f64,
    pub expected_db: f64,
    pub curve: FadeCurve,
    pub duration: Duration,
    pub started_at: Instant,
    /// Override threshold for this channel — at least OVERRIDE_THRESHOLD_DB,
    /// but widened to 1.5x the per-tick step size so hardware quantization
    /// echoes at extreme gain levels (e.g. -144 dB) don't trigger false overrides.
    pub override_threshold_db: f64,
}

impl ActiveChannel {
    pub(crate) fn new(
        group: i32,
        channel: i32,
        start_db: f64,
        target_db: f64,
        curve: FadeCurve,
        duration: Duration,
        started_at: Instant,
    ) -> Self {
        let total_db = (target_db - start_db).abs();
        let ticks = (duration.as_secs_f64() * TICK_HZ as f64).max(1.0);
        let step_db = total_db / ticks;
        let override_threshold_db = OVERRIDE_THRESHOLD_DB.max(step_db * 1.5);
        Self {
            group,
            channel,
            start_db,
            target_db,
            expected_db: start_db,
            curve,
            duration,
            started_at,
            override_threshold_db,
        }
    }

    /// Returns the interpolated dB value at `now`.
    pub(crate) fn value_at(&self, now: Instant) -> f64 {
        let elapsed = now.duration_since(self.started_at).as_secs_f64();
        let t = elapsed / self.duration.as_secs_f64();
        interpolate(self.start_db, self.target_db, t, self.curve)
    }

    /// Returns true if the fade has completed (t >= 1.0).
    pub(crate) fn is_done(&self, now: Instant) -> bool {
        now.duration_since(self.started_at) >= self.duration
    }

    /// Returns true if `reported` deviates from `expected_db` by >= threshold.
    pub(crate) fn is_override(&self, reported_db: f64) -> bool {
        (reported_db - self.expected_db).abs() >= self.override_threshold_db
    }

    /// Returns Some(new_db) if the value has moved enough to warrant sending.
    pub(crate) fn next_send(&mut self, now: Instant) -> Option<f64> {
        let new_db = if self.is_done(now) {
            self.target_db
        } else {
            self.value_at(now)
        };

        if (new_db - self.expected_db).abs() >= MIN_SEND_DELTA_DB {
            self.expected_db = new_db;
            Some(new_db)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FadeEngineHandle {
    tx: mpsc::Sender<FadeCommand>,
}

impl FadeEngineHandle {
    pub async fn start_fade(&self, config: FadeConfig) {
        let _ = self.tx.send(FadeCommand::StartFade { config }).await;
    }

    pub async fn abort_all(&self) {
        let _ = self.tx.send(FadeCommand::AbortAll).await;
    }

    pub async fn finish_now(&self) {
        let _ = self.tx.send(FadeCommand::FinishNow).await;
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
        self.subscribers.retain(|tx| tx.try_send(event.clone()).is_ok());
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
                Some(interval) => { interval.tick().await; true }
                None => { std::future::pending::<bool>().await }
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
                            lv1.set_gain(ch.group, ch.channel, ch.target_db).await;
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
                        lv1.set_gain(ch.group, ch.channel, new_db).await;
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
                    Some(crate::lv1::state::Lv1Event::FaderChanged { group, channel, gain_db }) => {
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
                    Some(crate::lv1::state::Lv1Event::Disconnected) => {
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
    use std::time::Duration;

    fn make_channel(start_db: f64, target_db: f64, duration_ms: u64) -> ActiveChannel {
        ActiveChannel::new(
            0, 0, start_db, target_db,
            FadeCurve::LinearDb,
            Duration::from_millis(duration_ms),
            Instant::now(),
        )
    }

    #[test]
    fn value_at_start_is_start_db() {
        let ch = make_channel(-20.0, -10.0, 4000);
        let v = ch.value_at(ch.started_at);
        assert!((v - -20.0).abs() < 1e-10);
    }

    #[test]
    fn value_at_end_is_target_db() {
        let ch = make_channel(-20.0, -10.0, 4000);
        let end = ch.started_at + Duration::from_millis(4000);
        let v = ch.value_at(end);
        assert!((v - -10.0).abs() < 1e-10);
    }

    #[test]
    fn is_done_false_before_duration() {
        let ch = make_channel(-20.0, -10.0, 4000);
        let mid = ch.started_at + Duration::from_millis(2000);
        assert!(!ch.is_done(mid));
    }

    #[test]
    fn is_done_true_at_duration() {
        let ch = make_channel(-20.0, -10.0, 4000);
        let end = ch.started_at + Duration::from_millis(4000);
        assert!(ch.is_done(end));
    }

    #[test]
    fn is_override_true_when_deviation_exceeds_threshold() {
        // Small fade: step = 10/100 = 0.1 dB/tick, threshold = max(0.5, 0.15) = 0.5 dB
        let ch = make_channel(-20.0, -10.0, 4000);
        assert!(ch.is_override(-20.0 + ch.override_threshold_db + 0.1));
    }

    #[test]
    fn is_override_false_when_deviation_below_threshold() {
        // Small fade: threshold = 0.5 dB
        let ch = make_channel(-20.0, -10.0, 4000);
        assert!(!ch.is_override(-20.0 + ch.override_threshold_db - 0.1));
    }

    #[test]
    fn override_threshold_widens_for_large_range_fade() {
        // -144 → 0 over 4s: step = 144 / (4*25) = 1.44 dB/tick, threshold = max(0.5, 2.16) = 2.16 dB
        let ch = make_channel(-144.0, 0.0, 4000);
        let expected_step = 144.0 / (4.0 * TICK_HZ as f64);
        let expected_threshold = OVERRIDE_THRESHOLD_DB.max(expected_step * 1.5);
        assert!((ch.override_threshold_db - expected_threshold).abs() < 1e-10);
        // A 1.5 dB echo deviation should NOT trigger override (within threshold)
        assert!(!ch.is_override(-144.0 + 1.5));
        // A 3.0 dB deviation SHOULD trigger override
        assert!(ch.is_override(-144.0 + expected_threshold + 0.1));
    }

    #[test]
    fn next_send_returns_none_when_below_min_delta() {
        let mut ch = make_channel(-20.0, -10.0, 4000);
        // At t=0, value is -20.0 = expected_db, delta is 0 — no send
        let now = ch.started_at;
        assert!(ch.next_send(now).is_none());
    }

    #[test]
    fn next_send_returns_some_when_above_min_delta() {
        let mut ch = make_channel(-20.0, -10.0, 4000);
        // At t=1.0 (end), value is -10.0, delta from -20.0 is 10.0 >= 0.1
        let end = ch.started_at + Duration::from_millis(4000);
        let result = ch.next_send(end);
        assert!(result.is_some());
        assert!((result.unwrap() - -10.0).abs() < 1e-10);
    }

    #[test]
    fn next_send_updates_expected_db() {
        let mut ch = make_channel(-20.0, -10.0, 4000);
        let end = ch.started_at + Duration::from_millis(4000);
        ch.next_send(end);
        assert!((ch.expected_db - -10.0).abs() < 1e-10);
    }

    #[test]
    fn next_send_at_done_returns_exact_target() {
        let mut ch = make_channel(-20.0, -10.0, 4000);
        let end = ch.started_at + Duration::from_millis(5000);
        let result = ch.next_send(end).unwrap();
        assert!((result - -10.0).abs() < 1e-10);
    }

    // Integration tests for the actor
    use crate::lv1::state::spawn_actor;
    use crate::lv1::tcp::encode_frame;
    use crate::osc::OscArg;
    use std::io::Write;
    use std::net::TcpListener;

    fn lv1_frame(address: &str, args: &[OscArg]) -> Vec<u8> {
        encode_frame(address, args).unwrap()
    }

    async fn wait_for_fade_event(
        events: &mut mpsc::Receiver<FadeEvent>,
        timeout: std::time::Duration,
        pred: impl Fn(&FadeEvent) -> bool,
    ) -> FadeEvent {
        tokio::time::timeout(timeout, async {
            while let Some(e) = events.recv().await {
                if pred(&e) { return e; }
            }
            panic!("event stream ended without matching event");
        }).await.expect("timed out waiting for fade event")
    }

    #[tokio::test]
    async fn engine_emits_fade_started_and_completed() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
            // Send a /Channels batch with one channel so start values are available
            let channels_args = {
                let mut a = vec![OscArg::Int(1)];
                a.push(OscArg::String("Ch 1".to_string()));
                a.push(OscArg::Int(0)); // group
                a.push(OscArg::Int(0)); // channel
                a.push(OscArg::Double(-8.0)); // gain_db
                for _ in 0..15 { a.push(OscArg::Int(0)); }
                a
            };
            stream.write_all(&lv1_frame("/Channels", &channels_args)).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(3));
        });

        let lv1 = spawn_actor("127.0.0.1".to_string(), port);
        let engine = spawn_engine(lv1);
        let mut fade_events = engine.subscribe().await;

        // Wait a moment for /Channels to be processed
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        engine.start_fade(FadeConfig {
            targets: vec![FadeTarget { group: 0, channel: 0, target_db: -10.0 }],
            duration_ms: 500,
            curve: FadeCurve::LinearDb,
        }).await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_millis(500),
            |e| matches!(e, FadeEvent::FadeStarted),
        ).await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_secs(3),
            |e| matches!(e, FadeEvent::FadeCompleted),
        ).await;
    }

    #[tokio::test]
    async fn engine_abort_all_stops_fade() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
            let channels_args = {
                let mut a = vec![OscArg::Int(1)];
                a.push(OscArg::String("Ch 1".to_string()));
                a.push(OscArg::Int(0));
                a.push(OscArg::Int(0));
                a.push(OscArg::Double(-8.0));
                for _ in 0..15 { a.push(OscArg::Int(0)); }
                a
            };
            stream.write_all(&lv1_frame("/Channels", &channels_args)).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(5));
        });

        let lv1 = spawn_actor("127.0.0.1".to_string(), port);
        let engine = spawn_engine(lv1);
        let mut fade_events = engine.subscribe().await;

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        engine.start_fade(FadeConfig {
            targets: vec![FadeTarget { group: 0, channel: 0, target_db: -30.0 }],
            duration_ms: 10_000,
            curve: FadeCurve::LinearDb,
        }).await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_millis(500),
            |e| matches!(e, FadeEvent::FadeStarted),
        ).await;

        engine.abort_all().await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_secs(2),
            |e| matches!(e, FadeEvent::FadeAborted),
        ).await;
    }

    #[tokio::test]
    async fn engine_detects_manual_override() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
            let channels_args = {
                let mut a = vec![OscArg::Int(1)];
                a.push(OscArg::String("Ch 1".to_string()));
                a.push(OscArg::Int(0));
                a.push(OscArg::Int(0));
                a.push(OscArg::Double(-8.0));
                for _ in 0..15 { a.push(OscArg::Int(0)); }
                a
            };
            stream.write_all(&lv1_frame("/Channels", &channels_args)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(400));

            // Simulate a large unexpected fader move (override)
            stream.write_all(&lv1_frame(
                "/Notify/Track/Out/Gain",
                &[OscArg::Int(0), OscArg::Int(0), OscArg::Double(0.0), OscArg::True],
            )).unwrap();

            std::thread::sleep(std::time::Duration::from_secs(3));
        });

        let lv1 = spawn_actor("127.0.0.1".to_string(), port);
        let engine = spawn_engine(lv1);
        let mut fade_events = engine.subscribe().await;

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        engine.start_fade(FadeConfig {
            targets: vec![FadeTarget { group: 0, channel: 0, target_db: -20.0 }],
            duration_ms: 10_000,
            curve: FadeCurve::LinearDb,
        }).await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_millis(500),
            |e| matches!(e, FadeEvent::FadeStarted),
        ).await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_secs(3),
            |e| matches!(e, FadeEvent::ChannelOverride { .. }),
        ).await;
    }

    #[tokio::test]
    async fn start_fade_while_running_replaces_previous() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&lv1_frame("/handshake", &[OscArg::Int(1)])).unwrap();
            let channels_args = {
                let mut a = vec![OscArg::Int(1)];
                a.push(OscArg::String("Ch 1".to_string()));
                a.push(OscArg::Int(0));
                a.push(OscArg::Int(0));
                a.push(OscArg::Double(-8.0));
                for _ in 0..15 { a.push(OscArg::Int(0)); }
                a
            };
            stream.write_all(&lv1_frame("/Channels", &channels_args)).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(5));
        });

        let lv1 = spawn_actor("127.0.0.1".to_string(), port);
        let engine = spawn_engine(lv1);
        let mut fade_events = engine.subscribe().await;

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // First fade — very long
        engine.start_fade(FadeConfig {
            targets: vec![FadeTarget { group: 0, channel: 0, target_db: -30.0 }],
            duration_ms: 30_000,
            curve: FadeCurve::LinearDb,
        }).await;

        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_millis(500),
            |e| matches!(e, FadeEvent::FadeStarted),
        ).await;

        // Second fade — short, replaces first
        engine.start_fade(FadeConfig {
            targets: vec![FadeTarget { group: 0, channel: 0, target_db: -10.0 }],
            duration_ms: 500,
            curve: FadeCurve::LinearDb,
        }).await;

        // Should get another FadeStarted (for the second fade)
        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_millis(500),
            |e| matches!(e, FadeEvent::FadeStarted),
        ).await;

        // Should complete the second fade
        wait_for_fade_event(
            &mut fade_events,
            std::time::Duration::from_secs(3),
            |e| matches!(e, FadeEvent::FadeCompleted),
        ).await;
    }
}

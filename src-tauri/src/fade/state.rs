use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::fade::commands::{
    RecallReadinessCancellation, RecallReadinessError, RecallReadinessRequest,
};
use crate::fade::events::FadeEvent;
use crate::fade::tick::ActiveTarget;
use crate::fade::types::FadeSceneIdentity;
use crate::runtime::events::AppEventBus;

pub(super) const READINESS_PINGS_REQUIRED: u8 = 2;

struct ReadinessBarrier {
    generation: u64,
    scene_index: i32,
    scene_name: String,
    last_counted_ping_sequence: u64,
    observed_ping_count: u8,
    missed_events: bool,
    deadline: tokio::time::Instant,
    completion: Option<oneshot::Sender<Result<(), RecallReadinessError>>>,
    timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PingGateProgress {
    Ignored,
    Waiting { observed: u8 },
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReadinessTimeoutContext {
    pub(super) generation: u64,
    pub(super) scene_index: i32,
    pub(super) scene_name: String,
    pub(super) observed_ping_count: u8,
    pub(super) completion_owned: bool,
    pub(super) timeout_ms: u64,
}

pub(crate) struct EngineState {
    generation: u64,
    pub(crate) channels: Vec<ActiveTarget>,
    pub(crate) event_bus: AppEventBus,
    readiness_barrier: Option<ReadinessBarrier>,
}

impl EngineState {
    pub(crate) fn new(event_bus: AppEventBus, generation: u64) -> Self {
        Self {
            generation,
            channels: Vec::new(),
            event_bus,
            readiness_barrier: None,
        }
    }

    pub(crate) fn fan_out(&mut self, event: FadeEvent) {
        self.event_bus.publish_fade(self.generation, event);
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.channels.is_empty()
    }

    pub(super) fn finish_scene_on_next_tick(&mut self, scene: &FadeSceneIdentity) -> usize {
        let mut count = 0;
        for target in &mut self.channels {
            if &target.scene == scene {
                target.finish_on_next_tick();
                count += 1;
            }
        }
        count
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn start_or_reset_readiness(
        &mut self,
        generation: u64,
        scene_index: i32,
        scene_name: String,
        ping_sequence: u64,
        now: Instant,
        readiness: RecallReadinessRequest,
    ) {
        let RecallReadinessRequest {
            deadline,
            completion,
        } = readiness;
        if let Some(previous) = self.readiness_barrier.take()
            && let Some(completion) = previous.completion
        {
            let _ = completion.send(Err(RecallReadinessError::Cancelled(
                RecallReadinessCancellation::Superseded,
            )));
        }
        for channel in &mut self.channels {
            channel.pause(now);
        }

        self.readiness_barrier = Some(ReadinessBarrier {
            generation,
            scene_index,
            scene_name,
            last_counted_ping_sequence: ping_sequence,
            observed_ping_count: 0,
            missed_events: false,
            deadline,
            timeout_ms: deadline
                .saturating_duration_since(tokio::time::Instant::now())
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
            completion,
        });
    }

    pub(super) fn mark_readiness_lagged(&mut self) {
        if let Some(barrier) = self.readiness_barrier.as_mut() {
            barrier.missed_events = true;
        }
    }

    pub(super) fn observe_ping(
        &mut self,
        generation: u64,
        sequence: u64,
        now: Instant,
    ) -> PingGateProgress {
        let Some(barrier) = self.readiness_barrier.as_mut() else {
            return PingGateProgress::Ignored;
        };

        if now >= barrier.deadline
            || barrier.missed_events
            || generation != barrier.generation
            || sequence <= barrier.last_counted_ping_sequence
        {
            return PingGateProgress::Ignored;
        }

        barrier.last_counted_ping_sequence = sequence;
        barrier.observed_ping_count = barrier.observed_ping_count.saturating_add(1);
        if barrier.observed_ping_count < READINESS_PINGS_REQUIRED {
            return PingGateProgress::Waiting {
                observed: barrier.observed_ping_count,
            };
        }

        let barrier = self
            .readiness_barrier
            .take()
            .expect("readiness barrier must remain present until release");
        for channel in &mut self.channels {
            channel.resume(now);
        }
        if let Some(completion) = barrier.completion {
            let _ = completion.send(Ok(()));
        }
        PingGateProgress::Released
    }

    pub(super) fn readiness_deadline(&self) -> Option<tokio::time::Instant> {
        self.readiness_barrier
            .as_ref()
            .map(|barrier| barrier.deadline)
    }

    pub(super) fn timeout_readiness(&mut self) -> Option<ReadinessTimeoutContext> {
        self.readiness_barrier.take().map(|barrier| {
            let context = ReadinessTimeoutContext {
                generation: barrier.generation,
                scene_index: barrier.scene_index,
                scene_name: barrier.scene_name.clone(),
                observed_ping_count: barrier.observed_ping_count,
                completion_owned: barrier.completion.is_some(),
                timeout_ms: barrier.timeout_ms,
            };
            if let Some(completion) = barrier.completion {
                let _ = completion.send(Err(RecallReadinessError::TimedOut {
                    generation: context.generation,
                    scene_index: context.scene_index,
                    scene_name: context.scene_name.clone(),
                    observed_ping_count: context.observed_ping_count,
                }));
            }
            self.channels.clear();
            context
        })
    }

    pub(super) fn is_waiting_for_readiness(&self) -> bool {
        self.readiness_barrier.is_some()
    }

    pub(crate) fn cancel_all_in_place(&mut self, cancellation: RecallReadinessCancellation) {
        self.channels.clear();
        if let Some(barrier) = self.readiness_barrier.take()
            && let Some(completion) = barrier.completion
        {
            let _ = completion.send(Err(RecallReadinessError::Cancelled(cancellation)));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use tokio::time::Instant;

    use super::*;
    use crate::fade::curve::FadeCurve;
    use crate::fade::tick::ActiveTargetInit;
    use crate::fade::types::{FadeParameter, FadeSceneIdentity, FadeTargetKey};

    fn active_target(
        started_at: Instant,
        scene: FadeSceneIdentity,
        channel: i32,
        target_value: f64,
    ) -> ActiveTarget {
        ActiveTarget::new(ActiveTargetInit {
            scene,
            key: FadeTargetKey {
                group: 0,
                channel,
                parameter: FadeParameter::FaderDb,
            },
            group: 0,
            channel,
            start_value: -20.0,
            target_value,
            curve: FadeCurve::Linear,
            duration: Duration::from_secs(1),
            started_at,
            expected_generation: None,
        })
    }

    #[test]
    fn readiness_requires_two_newer_same_generation_pings_before_releasing() {
        let now = Instant::now();
        let readiness_now = tokio::time::Instant::now();
        let mut state = EngineState::new(AppEventBus::default(), 4);

        let deadline = readiness_now + Duration::from_secs(5);
        state.start_or_reset_readiness(
            4,
            17,
            "Verse".to_string(),
            10,
            now,
            RecallReadinessRequest::detached(deadline),
        );

        assert_eq!(state.generation(), 4);
        assert!(state.is_waiting_for_readiness());
        assert_eq!(state.readiness_deadline(), Some(deadline));
        assert_eq!(state.observe_ping(4, 10, now), PingGateProgress::Ignored);
        assert_eq!(
            state.observe_ping(4, 11, now),
            PingGateProgress::Waiting { observed: 1 }
        );
        assert_eq!(state.observe_ping(4, 11, now), PingGateProgress::Ignored);
        assert_eq!(state.observe_ping(3, 12, now), PingGateProgress::Ignored);
        assert_eq!(state.observe_ping(4, 12, now), PingGateProgress::Released);
        assert!(!state.is_waiting_for_readiness());
        assert_eq!(state.readiness_deadline(), None);
    }

    #[test]
    fn readiness_reset_uses_new_boundary_and_preserves_original_pause() {
        let now = Instant::now();
        let mut state = EngineState::new(AppEventBus::default(), 4);
        state.channels.push(active_target(
            now,
            FadeSceneIdentity {
                index: 17,
                name: "Verse".to_string(),
            },
            0,
            -10.0,
        ));

        state.start_or_reset_readiness(
            4,
            17,
            "Verse".to_string(),
            10,
            now + Duration::from_millis(100),
            RecallReadinessRequest::detached(tokio::time::Instant::now() + Duration::from_secs(5)),
        );
        state.start_or_reset_readiness(
            4,
            18,
            "Chorus".to_string(),
            20,
            now + Duration::from_millis(200),
            RecallReadinessRequest::detached(tokio::time::Instant::now() + Duration::from_secs(5)),
        );

        assert!(state.channels[0].is_paused());
        assert_eq!(
            state.observe_ping(4, 20, now + Duration::from_millis(300)),
            PingGateProgress::Ignored
        );
        assert_eq!(
            state.observe_ping(4, 21, now + Duration::from_millis(400)),
            PingGateProgress::Waiting { observed: 1 }
        );

        assert_eq!(
            state.observe_ping(4, 22, now + Duration::from_millis(1_100)),
            PingGateProgress::Released
        );
        assert!(!state.channels[0].is_paused());
        assert_eq!(state.channels[0].started_at, now + Duration::from_secs(1));
    }

    #[test]
    fn cancellation_clears_targets_and_readiness_barrier_together() {
        let now = Instant::now();
        let mut state = EngineState::new(AppEventBus::default(), 4);
        state.channels.push(active_target(
            now,
            FadeSceneIdentity {
                index: 17,
                name: "Verse".to_string(),
            },
            0,
            -10.0,
        ));
        state.start_or_reset_readiness(
            4,
            17,
            "Verse".to_string(),
            10,
            now,
            RecallReadinessRequest::detached(tokio::time::Instant::now() + Duration::from_secs(5)),
        );

        state.cancel_all_in_place(RecallReadinessCancellation::Aborted);

        assert!(state.channels.is_empty());
        assert!(!state.is_waiting_for_readiness());
        assert_eq!(state.readiness_deadline(), None);
    }

    #[test]
    fn finish_scene_rewrites_only_exact_scene_owner() {
        let now = Instant::now();
        let scene_a = FadeSceneIdentity {
            index: 17,
            name: "Verse".to_string(),
        };
        let same_index_wrong_name = FadeSceneIdentity {
            index: 17,
            name: "Verse Copy".to_string(),
        };
        let scene_b = FadeSceneIdentity {
            index: 18,
            name: "Chorus".to_string(),
        };
        let mut state = EngineState::new(AppEventBus::default(), 4);
        state
            .channels
            .push(active_target(now, scene_a.clone(), 1, -10.0));
        state
            .channels
            .push(active_target(now, scene_a.clone(), 2, -12.0));
        state
            .channels
            .push(active_target(now, same_index_wrong_name, 3, -14.0));
        state.channels.push(active_target(now, scene_b, 4, -16.0));

        assert_eq!(state.finish_scene_on_next_tick(&scene_a), 2);
        assert!(state.channels[0].is_done(now));
        assert!(state.channels[1].is_done(now));
        assert!(!state.channels[2].is_done(now));
        assert!(!state.channels[3].is_done(now));
        assert_eq!(state.channels[0].target_value, -10.0);
        assert_eq!(state.channels[1].target_value, -12.0);
    }

    #[test]
    fn finish_scene_returns_zero_without_mutating_unowned_targets() {
        let now = Instant::now();
        let mut state = EngineState::new(AppEventBus::default(), 4);
        state.channels.push(active_target(
            now,
            FadeSceneIdentity {
                index: 18,
                name: "Chorus".to_string(),
            },
            1,
            -10.0,
        ));

        let count = state.finish_scene_on_next_tick(&FadeSceneIdentity {
            index: 17,
            name: "Verse".to_string(),
        });

        assert_eq!(count, 0);
        assert!(!state.channels[0].is_done(now));
    }
}

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

    /// @cc [owner:mixxorz,label:architecture;safety] generation-tagged-publication
    /// Every fade fact emitted by this engine state MUST carry the generation fixed when the state
    /// was constructed; callers MUST NOT supply or retag a publication generation.
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

    /// @cc [owner:mixxorz,label:product;safety] readiness-reset-pauses
    /// Starting or replacing readiness MUST pause every active target at the first pause boundary,
    /// replace the prior barrier, and complete any prior owned waiter as `Superseded`; elapsed
    /// readiness time MUST NOT advance target interpolation.
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

    /// @cc [owner:mixxorz,label:safety] readiness-release
    /// Readiness MUST release only after two strictly newer ping sequences from the barrier's exact
    /// generation arrive before its absolute deadline with no subscriber lag. Ignored pings MUST
    /// neither advance the count nor resume targets; release MUST resume all targets and complete an
    /// owned waiter successfully.
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

    /// @cc [owner:mixxorz,label:safety;reliability] readiness-timeout
    /// Timing out an installed barrier MUST clear every active target and complete its owned waiter
    /// with the barrier's generation, exact scene identity, and observed ping count; no barrier MUST
    /// be a no-op.
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

    /// @cc [owner:mixxorz,label:safety] abort-clears-all
    /// Cancellation MUST synchronously remove all active targets and the readiness barrier, and MUST
    /// complete an owned readiness waiter with the supplied cancellation reason so later ticks or
    /// pings cannot revive the canceled work.
    pub(crate) fn cancel_all_in_place(&mut self, cancellation: RecallReadinessCancellation) {
        self.channels.clear();
        if let Some(barrier) = self.readiness_barrier.take()
            && let Some(completion) = barrier.completion
        {
            let _ = completion.send(Err(RecallReadinessError::Cancelled(cancellation)));
        }
    }
}

use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::fade::commands::{
    RecallReadinessCancellation, RecallReadinessError, RecallReadinessRequest,
};
use crate::fade::events::FadeEvent;
use crate::fade::tick::ActiveTarget;
use crate::fade::types::{FadeSceneIdentity, FadeTargetKey};
use crate::runtime::events::AppEventBus;

pub(super) const READINESS_PINGS_REQUIRED: u8 = 2;

struct ReadinessBarrier {
    scene_index: i32,
    scene_name: String,
    progress: ReadinessProgress,
    deadline: tokio::time::Instant,
    completion: Option<oneshot::Sender<Result<(), RecallReadinessError>>>,
    timeout_ms: u64,
}

/// Stored readiness phases. Lag is terminal until cancellation/replacement/timeout.
#[derive(Debug, Clone, Copy)]
enum ReadinessProgress {
    AwaitingFirst { after: u64 },
    AwaitingSecond { after: u64 },
    Lagged { observed: u8 },
}

impl ReadinessProgress {
    fn observed(self) -> u8 {
        match self {
            Self::AwaitingFirst { .. } => 0,
            Self::AwaitingSecond { .. } => 1,
            Self::Lagged { observed } => observed,
        }
    }

    fn observe(&mut self, sequence: u64) -> PingGateProgress {
        match *self {
            Self::AwaitingFirst { after } if sequence > after => {
                *self = Self::AwaitingSecond { after: sequence };
                PingGateProgress::Waiting { observed: 1 }
            }
            Self::AwaitingSecond { after } if sequence > after => PingGateProgress::Released,
            _ => PingGateProgress::Ignored,
        }
    }

    fn lag(&mut self) {
        *self = Self::Lagged { observed: self.observed() };
    }
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

    /// Key ownership transitions are atomic: installing replaces only that key;
    /// removing ends its running/paused target lifecycle without touching other owners.
    pub(super) fn install_target(&mut self, target: ActiveTarget) {
        self.channels.retain(|active| active.key != target.key);
        self.channels.push(target);
    }

    pub(super) fn release_target(&mut self, key: &FadeTargetKey) {
        self.channels.retain(|target| &target.key != key);
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
            scene_index,
            scene_name,
            progress: ReadinessProgress::AwaitingFirst { after: ping_sequence },
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
            barrier.progress.lag();
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

        if now >= barrier.deadline || generation != self.generation {
            return PingGateProgress::Ignored;
        }
        match barrier.progress.observe(sequence) {
            PingGateProgress::Released => {}
            progress => return progress,
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
                generation: self.generation,
                scene_index: barrier.scene_index,
                scene_name: barrier.scene_name.clone(),
                observed_ping_count: barrier.progress.observed(),
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

    /// @cc [owner:mixxorz,label:safety] safety-cancellation-clears-work
    /// Runtime safety cancellation MUST synchronously remove all active targets and the readiness
    /// barrier, and MUST complete an owned readiness waiter with the supplied reason so later ticks
    /// or pings cannot revive the canceled work.
    pub(crate) fn cancel_runtime_work(&mut self, cancellation: RecallReadinessCancellation) {
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
    use super::*;

    #[test]
    fn readiness_requires_two_newer_sequences_and_lag_cannot_release() {
        let mut progress = ReadinessProgress::AwaitingFirst { after: 10 };
        assert_eq!(progress.observe(10), PingGateProgress::Ignored);
        assert_eq!(progress.observe(11), PingGateProgress::Waiting { observed: 1 });
        assert_eq!(progress.observe(11), PingGateProgress::Ignored);
        let mut poisoned = progress;
        poisoned.lag();
        assert_eq!(poisoned.observe(12), PingGateProgress::Ignored);
        assert_eq!(poisoned.observed(), 1);
        assert_eq!(progress.observe(12), PingGateProgress::Released);
    }
}

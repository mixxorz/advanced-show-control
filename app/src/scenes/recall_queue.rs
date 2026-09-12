use std::{collections::VecDeque, time::Duration};

use tokio::sync::oneshot;
use tokio::time::Instant;
use uuid::Uuid;

use crate::fade::{RecallReadinessCancellation, RecallReadinessError};
use crate::runtime::errors::AppCommandError;

use super::RecallSceneResult;

pub(super) const RECALL_QUEUE_CAPACITY: usize = 8;
pub(super) const RECALL_COMPLETION_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) struct QueuedRecall {
    pub request_id: Uuid,
    pub internal_scene_id: Uuid,
    pub reply: oneshot::Sender<Result<RecallSceneResult, AppCommandError>>,
}

pub(super) enum InFlightPhase {
    AwaitingObservation {
        dispatch_sequence: u64,
        deadline: Instant,
    },
    AwaitingReadiness {
        completion: Option<oneshot::Receiver<Result<(), RecallReadinessError>>>,
    },
}

pub(super) struct InFlightRecall {
    pub request_id: Uuid,
    pub generation: u64,
    pub result: RecallSceneResult,
    pub phase: InFlightPhase,
}

pub(super) struct RecallReadinessCompletion {
    pub request_id: Uuid,
    pub generation: u64,
    pub result: Result<(), RecallReadinessError>,
}

#[derive(Default)]
pub(super) struct RecallQueue {
    pub in_flight: Option<InFlightRecall>,
    pub waiting: VecDeque<QueuedRecall>,
}

impl RecallQueue {
    /**
     * @cc [owner:mixxorz,label:safety] readiness-owned-by-in-flight-recall
     * Readiness completion MUST be polled only from the current in-flight recall's owned receiver;
     * absent or already-consumed readiness MUST remain pending, and receiver closure MUST surface
     * as cancellation rather than success.
     */
    pub async fn readiness_completion(&mut self) -> RecallReadinessCompletion {
        let Some(InFlightRecall {
            request_id,
            generation,
            phase: InFlightPhase::AwaitingReadiness { completion },
            ..
        }) = self.in_flight.as_mut()
        else {
            return std::future::pending().await;
        };
        let Some(receiver) = completion.as_mut() else {
            return std::future::pending().await;
        };
        let result = receiver
            .await
            .unwrap_or(Err(RecallReadinessError::Cancelled(
                RecallReadinessCancellation::ActorStopped,
            )));
        completion.take();
        RecallReadinessCompletion {
            request_id: *request_id,
            generation: *generation,
            result,
        }
    }

    /**
     * @cc [owner:mixxorz,label:product] recall-capacity-includes-in-flight
     * Queue occupancy MUST count both the in-flight request and waiting requests so total admitted
     * explicit recall intent never exceeds `RECALL_QUEUE_CAPACITY`.
     */
    pub fn len(&self) -> usize {
        self.waiting.len() + usize::from(self.in_flight.is_some())
    }

    pub fn is_full(&self) -> bool {
        self.len() >= RECALL_QUEUE_CAPACITY
    }

    pub fn admit(&mut self, recall: QueuedRecall) {
        self.waiting.push_back(recall);
    }

    /**
     * @cc [owner:mixxorz,label:product] explicit-recall-fifo
     * Waiting explicit recalls MUST be removed in admission order; repeated requests for the same
     * scene remain distinct queue entries.
     */
    pub fn take_next(&mut self) -> Option<QueuedRecall> {
        self.waiting.pop_front()
    }

    pub fn set_in_flight(&mut self, recall: InFlightRecall) {
        self.in_flight = Some(recall);
    }

    /**
     * @cc [owner:mixxorz,label:safety] cancel-waiting-replies
     * Draining MUST remove every waiting request and resolve each still-open caller reply with the
     * supplied cancellation error; it MUST NOT report any waiting request as dispatched.
     */
    pub fn drain_pending(&mut self, error: AppCommandError) {
        for queued in self.waiting.drain(..) {
            let _ = queued.reply.send(Err(error.clone()));
        }
    }
}

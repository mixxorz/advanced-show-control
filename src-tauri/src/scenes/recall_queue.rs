use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use tokio::sync::oneshot;
use uuid::Uuid;

use crate::runtime::errors::AppCommandError;

use super::RecallSceneResult;

pub(super) const RECALL_QUEUE_CAPACITY: usize = 8;
pub(super) const RECALL_COMPLETION_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) struct QueuedRecall {
    pub request_id: Uuid,
    pub internal_scene_id: Uuid,
    pub reply: oneshot::Sender<Result<RecallSceneResult, AppCommandError>>,
}

#[allow(dead_code)] // Task 5 consumes the observation and readiness phases.
pub(super) enum InFlightPhase {
    AwaitingObservation {
        dispatch_sequence: u64,
        deadline: Instant,
    },
    AwaitingReadiness {
        deadline: Instant,
    },
}

#[allow(dead_code)] // Task 5 consumes the stored completion metadata.
pub(super) struct InFlightRecall {
    pub request_id: Uuid,
    pub generation: u64,
    pub result: RecallSceneResult,
    pub phase: InFlightPhase,
}

#[derive(Default)]
pub(super) struct RecallQueue {
    pub in_flight: Option<InFlightRecall>,
    pub waiting: VecDeque<QueuedRecall>,
}

impl RecallQueue {
    pub fn len(&self) -> usize {
        self.waiting.len() + usize::from(self.in_flight.is_some())
    }

    pub fn is_full(&self) -> bool {
        self.len() >= RECALL_QUEUE_CAPACITY
    }

    pub fn admit(&mut self, recall: QueuedRecall) {
        self.waiting.push_back(recall);
    }

    pub fn take_next(&mut self) -> Option<QueuedRecall> {
        self.waiting.pop_front()
    }

    pub fn set_in_flight(&mut self, recall: InFlightRecall) {
        self.in_flight = Some(recall);
    }

    pub fn drain_pending(&mut self, error: AppCommandError) {
        for queued in self.waiting.drain(..) {
            let _ = queued.reply.send(Err(error.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draining_pending_recalls_cancels_each_waiting_reply() {
        let mut queue = RecallQueue::default();
        let (first_reply, mut first) = oneshot::channel();
        let (second_reply, mut second) = oneshot::channel();
        queue.admit(QueuedRecall {
            request_id: Uuid::new_v4(),
            internal_scene_id: Uuid::new_v4(),
            reply: first_reply,
        });
        queue.admit(QueuedRecall {
            request_id: Uuid::new_v4(),
            internal_scene_id: Uuid::new_v4(),
            reply: second_reply,
        });

        queue.drain_pending(AppCommandError::RecallCanceled(
            "LV1 recall command is unavailable".to_string(),
        ));

        assert_eq!(queue.len(), 0);
        assert_eq!(
            first.try_recv(),
            Ok(Err(AppCommandError::RecallCanceled(
                "LV1 recall command is unavailable".to_string()
            )))
        );
        assert_eq!(
            second.try_recv(),
            Ok(Err(AppCommandError::RecallCanceled(
                "LV1 recall command is unavailable".to_string()
            )))
        );
    }
}

use std::{collections::VecDeque, time::Duration};

use tokio::sync::oneshot;
use tokio::time::Instant;
use uuid::Uuid;

use crate::fade::{
    FadeCommand, FadeEngineHandle, FadeSceneIdentity, RecallReadinessCancellation,
    RecallReadinessError, RecallReadinessRequest, SameSceneRecallBehavior,
};
use crate::lv1::{
    ConnectionStatus, Lv1ActorError, Lv1Command, Lv1Connection, Lv1StateSnapshot, SceneState,
};
use crate::runtime::errors::AppCommandError;
use crate::runtime::events::AppEventBus;
use crate::scenes::policy::{RecallPolicyDecision, RecallPolicyInput, decide_scene_recall};
use crate::settings::AppSettings;
use crate::show::ShowLockoutReader;

use super::{RecallSceneResult, SceneDocument, ScenesEvent, ScenesState};

pub(super) const RECALL_QUEUE_CAPACITY: usize = 8;
pub(super) const RECALL_READINESS_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const SCENE_CHANGED_SETTLE_DELAY: Duration = Duration::from_millis(25);
pub(super) const LATE_CANCELED_OBSERVATION_CAPACITY: usize = 8;
pub(super) const LATE_CANCELED_OBSERVATION_TTL: Duration = Duration::from_secs(5);

pub(super) struct QueuedRecall {
    request_id: Uuid,
    internal_scene_id: Uuid,
    reply: oneshot::Sender<Result<RecallSceneResult, AppCommandError>>,
}

enum InFlightPhase {
    AwaitingObservation {
        dispatch_sequence: u64,
        deadline: Instant,
    },
    AwaitingReadiness {
        completion: Option<oneshot::Receiver<Result<(), RecallReadinessError>>>,
    },
    PostReadinessInterval {
        until: Instant,
    },
}

struct InFlightRecall {
    request_id: Uuid,
    generation: u64,
    result: RecallSceneResult,
    phase: InFlightPhase,
}

pub(super) struct RecallReadinessCompletion {
    request_id: Uuid,
    generation: u64,
    result: Result<(), RecallReadinessError>,
}

#[derive(Clone, Copy)]
pub(super) struct RecallDeadline {
    request_id: Uuid,
    at: Instant,
    kind: RecallDeadlineKind,
}

#[derive(Clone, Copy)]
enum RecallDeadlineKind {
    Safety,
    Interval,
}

impl RecallDeadline {
    pub fn at(&self) -> Instant {
        self.at
    }
}

impl RecallReadinessCompletion {
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[derive(Clone, Copy)]
struct QueueReadiness {
    request_id: Uuid,
    generation: u64,
    deadline: Instant,
}

pub(super) struct PendingSceneObservation {
    generation: u64,
    sequence: u64,
    scene: SceneState,
    seen_at: Instant,
    settle_after: Instant,
}

impl PendingSceneObservation {
    fn new(generation: u64, sequence: u64, scene: SceneState, now: Instant) -> Self {
        Self {
            generation,
            sequence,
            scene,
            seen_at: now,
            settle_after: now + SCENE_CHANGED_SETTLE_DELAY,
        }
    }
}

struct LateCanceledObservation {
    generation: u64,
    dispatch_sequence: u64,
    scene: SceneState,
    expires_at: Instant,
}

#[derive(Default)]
struct LateCanceledObservations {
    entries: Vec<LateCanceledObservation>,
    suppress_all_until: Option<(u64, Instant)>,
}

#[derive(Clone, Copy)]
enum LateCanceledObservationSuppression {
    Exact,
    Fallback,
}

impl LateCanceledObservations {
    fn retain(&mut self, in_flight: InFlightRecall) {
        let InFlightPhase::AwaitingObservation {
            dispatch_sequence, ..
        } = in_flight.phase
        else {
            return;
        };
        self.record(
            in_flight.generation,
            dispatch_sequence,
            SceneState {
                index: in_flight.result.lv1_scene_index,
                name: in_flight.result.scene.scene_name,
            },
            Instant::now(),
        );
    }

    /**
     * @cc [owner:mixxorz,label:safety] late-cancellation-overflow-fails-closed
     * Exact late-observation suppression MUST retain at most eight entries. Recording beyond that
     * capacity MUST activate generation-specific suppression of all otherwise spontaneous scene
     * observations for five seconds; exact matching of a current queued recall MUST remain eligible
     * before this fallback is consulted.
     */
    fn record(&mut self, generation: u64, dispatch_sequence: u64, scene: SceneState, now: Instant) {
        self.purge(now);
        let expires_at = now + LATE_CANCELED_OBSERVATION_TTL;
        if self.entries.len() >= LATE_CANCELED_OBSERVATION_CAPACITY {
            self.suppress_all_until = Some(match self.suppress_all_until {
                Some((current_generation, current_until)) if current_generation == generation => {
                    (generation, current_until.max(expires_at))
                }
                _ => (generation, expires_at),
            });
            return;
        }
        self.entries.push(LateCanceledObservation {
            generation,
            dispatch_sequence,
            scene,
            expires_at,
        });
    }

    fn suppresses(
        &mut self,
        observation: &PendingSceneObservation,
        now: Instant,
    ) -> Option<LateCanceledObservationSuppression> {
        self.purge(now);
        if matches!(
            self.suppress_all_until,
            Some((generation, until)) if generation == observation.generation && now < until
        ) {
            return Some(LateCanceledObservationSuppression::Fallback);
        }
        let index = self.entries.iter().position(|entry| {
            entry.generation == observation.generation
                && observation.sequence > entry.dispatch_sequence
                && entry.scene == observation.scene
        })?;
        self.entries.remove(index);
        Some(LateCanceledObservationSuppression::Exact)
    }

    fn purge(&mut self, now: Instant) {
        self.entries.retain(|entry| entry.expires_at > now);
        if self
            .suppress_all_until
            .is_some_and(|(_, until)| until <= now)
        {
            self.suppress_all_until = None;
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.suppress_all_until = None;
    }
}

#[cfg(test)]
pub(super) struct BeforeFadeHandoff {
    pub reached: oneshot::Sender<()>,
    pub resume: oneshot::Receiver<()>,
}

/**
 * @cc [owner:mixxorz,label:architecture;safety] recall-coordinator-owns-runtime-state
 * The app-lifetime Scenes actor MUST own exactly one synchronous `RecallCoordinator`. All explicit
 * recall FIFO phases, pending scene observations, late-cancellation suppression, readiness
 * receivers, and post-readiness interval state MUST be mutated through this coordinator; it MUST
 * NOT spawn work or outlive the owning actor turn.
 */
#[derive(Default)]
pub(super) struct RecallCoordinator {
    in_flight: Option<InFlightRecall>,
    waiting: VecDeque<QueuedRecall>,
    pending_observation: Option<PendingSceneObservation>,
    late_canceled_observations: LateCanceledObservations,
}

impl RecallCoordinator {
    pub fn observe_scene(
        &mut self,
        generation: u64,
        sequence: u64,
        scene: SceneState,
        now: Instant,
    ) {
        self.pending_observation = Some(PendingSceneObservation::new(
            generation, sequence, scene, now,
        ));
    }

    pub fn pending_observation_deadline(&self) -> Option<Instant> {
        self.pending_observation
            .as_ref()
            .map(|observation| observation.settle_after)
    }

    pub fn take_pending_observation(&mut self) -> Option<PendingSceneObservation> {
        self.pending_observation.take()
    }

    pub fn clear_pending_observation(&mut self) {
        self.pending_observation = None;
    }

    pub fn clear_late_observations(&mut self) {
        self.late_canceled_observations.clear();
    }

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
    pub fn is_full(&self) -> bool {
        self.waiting.len() + usize::from(self.in_flight.is_some()) >= RECALL_QUEUE_CAPACITY
    }

    fn is_idle(&self) -> bool {
        self.in_flight.is_none()
    }

    /**
     * @cc [owner:mixxorz,label:product] explicit-recall-fifo
     * Waiting explicit recalls MUST be removed in admission order; repeated requests for the same
     * scene remain distinct queue entries.
     */
    fn take_next(&mut self) -> Option<QueuedRecall> {
        self.waiting.pop_front()
    }

    pub fn deadline(&self) -> Option<RecallDeadline> {
        let recall = self.in_flight.as_ref()?;
        let (at, kind) = match recall.phase {
            InFlightPhase::AwaitingObservation { deadline, .. } => {
                (deadline, RecallDeadlineKind::Safety)
            }
            InFlightPhase::AwaitingReadiness { .. } => return None,
            InFlightPhase::PostReadinessInterval { until } => (until, RecallDeadlineKind::Interval),
        };
        Some(RecallDeadline {
            request_id: recall.request_id,
            at,
            kind,
        })
    }

    /**
     * @cc [owner:mixxorz,label:product;safety] recall-deadline-transition
     * The owning actor MUST drive the coordinator's single current deadline. An elapsed safety
     * deadline MUST cancel remaining recall intent, while an elapsed post-readiness interval MUST
     * make the next FIFO request dispatchable. Stale or early deadline values MUST have no effect.
     * A nonzero interval remains current even when no request is waiting; zero is immediately
     * dispatchable after readiness.
     */
    pub fn handle_deadline(&mut self, elapsed: RecallDeadline, now: Instant) -> bool {
        if now < elapsed.at {
            return false;
        }
        let Some(in_flight) = self.in_flight.as_ref() else {
            return false;
        };
        if in_flight.request_id != elapsed.request_id {
            return false;
        }
        let matches_phase = match (elapsed.kind, &in_flight.phase) {
            (RecallDeadlineKind::Safety, InFlightPhase::AwaitingObservation { deadline, .. }) => {
                *deadline == elapsed.at
            }
            (RecallDeadlineKind::Interval, InFlightPhase::PostReadinessInterval { until }) => {
                *until == elapsed.at
            }
            _ => false,
        };
        if !matches_phase {
            return false;
        }
        match elapsed.kind {
            RecallDeadlineKind::Safety => {
                self.cancel("LV1 recall readiness was lost", true);
                false
            }
            RecallDeadlineKind::Interval => {
                self.in_flight = None;
                true
            }
        }
    }

    /**
     * @cc [owner:mixxorz,label:safety] readiness-completion-fenced
     * A readiness result MUST affect the coordinator only when its request and generation still
     * identify the awaiting in-flight recall and the owning actor has established that generation
     * as authoritative. Timeout or cancellation MUST cancel remaining intent; only success may
     * begin the post-readiness interval or make the next FIFO request dispatchable.
     */
    pub fn handle_readiness_completion(
        &mut self,
        completion: RecallReadinessCompletion,
        interval: Duration,
        now: Instant,
    ) -> bool {
        if !self.in_flight.as_ref().is_some_and(|in_flight| {
            in_flight.request_id == completion.request_id
                && in_flight.generation == completion.generation
                && matches!(in_flight.phase, InFlightPhase::AwaitingReadiness { .. })
        }) {
            return false;
        }

        match completion.result {
            Err(RecallReadinessError::TimedOut {
                generation,
                scene_index,
                scene_name,
                observed_ping_count,
            }) => {
                if self.cancel("LV1 recall readiness was lost", false) {
                    tracing::warn!(
                        event = "scene_recall_queue_cancelled",
                        generation,
                        scene_index,
                        scene_name = %scene_name,
                        observed_ping_count,
                        timeout_ms = RECALL_READINESS_TIMEOUT.as_millis(),
                        "Paused fades were aborted and queued scene recalls were canceled because LV1 did not resume its keepalive cadence after scene recall"
                    );
                }
                false
            }
            Err(_) => {
                self.cancel("LV1 recall readiness was lost", true);
                false
            }
            Ok(()) if interval.is_zero() => {
                self.in_flight = None;
                true
            }
            Ok(()) => {
                let in_flight = self
                    .in_flight
                    .as_mut()
                    .expect("matching readiness retains its in-flight recall");
                in_flight.phase = InFlightPhase::PostReadinessInterval {
                    until: now + interval,
                };
                false
            }
        }
    }

    /**
     * @cc [owner:mixxorz,label:safety] recall-cancellation-boundary
     * Cancellation MUST drop the in-flight readiness receiver, fail every waiting caller, and
     * retain an awaiting-observation identity for bounded late-event suppression. It MUST NOT
     * itself abort an active fade or turn an already-dispatched caller reply into a failure.
     */
    pub fn cancel(&mut self, reason: &str, emit_log: bool) -> bool {
        let in_flight = self.in_flight.take();
        let had_in_flight = in_flight.is_some();
        if let Some(in_flight) = in_flight {
            self.late_canceled_observations.retain(in_flight);
        }
        let had_waiting = !self.waiting.is_empty();
        for queued in self.waiting.drain(..) {
            let _ = queued
                .reply
                .send(Err(AppCommandError::RecallCanceled(reason.to_string())));
        }
        if emit_log && (had_in_flight || had_waiting) {
            tracing::warn!(
                event = "scene_recall_queue_cancelled",
                reason,
                "Queued scene recalls were canceled because {reason}"
            );
        }
        had_in_flight || had_waiting
    }

    #[allow(clippy::too_many_arguments)]
    /**
     * @cc [owner:mixxorz,label:safety] observation-fade-handoff-gates
     * Before every Fade admission, an accepted observation MUST be validated against fresh exact
     * LV1 state, current generation, current lockout, linked config, live topology, enabled scopes,
     * and required targets. The generation, lockout, and absolute readiness deadline MUST be
     * rechecked after mailbox reservation with no await before command admission.
     */
    /**
     * @cc [owner:mixxorz,label:safety] nonadmitted-recall-side-effects
     * A blocked, skipped, suppressed, stale, or disabled pre-admission observation MUST NOT send
     * `RecallSceneFade` or abort an active fade. A queued exact observation still MUST complete the
     * readiness handoff without converting a blocked or skipped policy outcome into fade admission.
     */
    pub async fn process_scene_observation(
        &mut self,
        lv1: &Lv1Connection,
        fade: &FadeEngineHandle,
        event_bus: &AppEventBus,
        recall_state: &mut ScenesState,
        settings: &AppSettings,
        lockout: &ShowLockoutReader,
        #[cfg(test)] before_fade_handoff: &mut Option<BeforeFadeHandoff>,
        observation: PendingSceneObservation,
    ) {
        let generation = lv1.generation();
        let now = Instant::now();
        let queue_readiness = self.exact_queue_readiness(&observation);
        if queue_readiness.is_some_and(|readiness| now >= readiness.deadline) {
            self.cancel("LV1 recall readiness was lost", true);
            return;
        }
        if queue_readiness.is_none()
            && let Some(suppression) = self
                .late_canceled_observations
                .suppresses(&observation, now)
        {
            tracing::debug!(
                event = "scene_recall_late_observation_suppressed",
                generation = observation.generation,
                scene_index = observation.scene.index,
                scene_name = %observation.scene.name,
                sequence = observation.sequence,
                suppression = match suppression {
                    LateCanceledObservationSuppression::Exact => "exact_late_observation",
                    LateCanceledObservationSuppression::Fallback => "bounded_fallback",
                },
                "Ignored a scene observation while canceled recall suppression is active"
            );
            return;
        }

        let skipped_reason = if recall_state.is_scene_list_edit_suppressed(observation.seen_at)
            || recall_state.is_scene_list_edit_suppressed(now)
        {
            Some("scene list edit suppression".to_string())
        } else {
            let same_scene_repeat_delay =
                Duration::from_millis(settings.same_scene_recall_threshold_ms);
            (!recall_state.accepts(&observation.scene, same_scene_repeat_delay))
                .then(|| "scene not accepted by recall policy".to_string())
        };
        if let Some(reason) = skipped_reason.as_deref()
            && queue_readiness.is_none()
        {
            let scene_label = scene_label(&observation.scene);
            tracing::debug!(event = "scene_recall_skipped", scene = %scene_label, reason = %reason, "Scene recall skipped for {scene_label}: {reason}");
            return;
        }

        if lv1.ensure_current().await.is_err() {
            return;
        }
        let lv1_snapshot = match fresh_lv1_snapshot(
            lv1,
            &observation.scene,
            queue_readiness.map(|readiness| readiness.deadline),
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(err) => {
                if queue_readiness.is_some_and(|readiness| Instant::now() >= readiness.deadline) {
                    self.cancel("LV1 recall readiness was lost", true);
                    return;
                }
                if lv1.ensure_current().await.is_err() {
                    return;
                }
                event_bus.publish_scenes(
                    generation,
                    ScenesEvent::Blocked {
                        scene_label: scene_label(&observation.scene),
                        reason: format!("LV1 state is unavailable: {err}"),
                    },
                );
                return;
            }
        };
        let scene_config = recall_state
            .scene_configs()
            .iter()
            .find(|scene| {
                scene.scene_index == Some(observation.scene.index)
                    && scene.scene_name == observation.scene.name
            })
            .cloned();
        let decision = if let Some(reason) = skipped_reason {
            RecallPolicyDecision::Skip { reason }
        } else {
            decide_scene_recall(RecallPolicyInput {
                recalled_scene: observation.scene.clone(),
                lv1_snapshot: lv1_snapshot.clone(),
                lockout: lockout.current(),
                scene_config: scene_config.clone(),
            })
        };
        let blocked = matches!(&decision, RecallPolicyDecision::Blocked { .. });

        match decision {
            RecallPolicyDecision::Start(initial_fade_config) => {
                #[cfg(test)]
                if let Some(BeforeFadeHandoff { reached, resume }) = before_fade_handoff.take() {
                    let _ = reached.send(());
                    let _ = resume.await;
                }

                let prepared = if let Some(queue_readiness) = queue_readiness {
                    let current_lockout = lockout.current();
                    let RecallPolicyDecision::Start(fade_config) =
                        decide_scene_recall(RecallPolicyInput {
                            recalled_scene: observation.scene.clone(),
                            lv1_snapshot,
                            lockout: current_lockout,
                            scene_config,
                        })
                    else {
                        self.cancel(
                            if current_lockout {
                                "lockout was enabled"
                            } else {
                                "LV1 recall readiness was lost"
                            },
                            true,
                        );
                        return;
                    };
                    let Some((readiness, completion)) =
                        self.prepare_readiness(queue_readiness, Instant::now())
                    else {
                        self.cancel("LV1 recall readiness was lost", true);
                        return;
                    };
                    (fade_config, readiness, Some((queue_readiness, completion)))
                } else {
                    (
                        initial_fade_config,
                        RecallReadinessRequest::detached(Instant::now() + RECALL_READINESS_TIMEOUT),
                        None,
                    )
                };
                let (fade_config, readiness, queued_readiness) = prepared;
                let scene_label = scene_label(&observation.scene);
                if lv1
                    .if_current(|| {
                        tracing::debug!(event = "scene_recall_ready", scene = %scene_label, target_count = fade_config.targets.len(), "Scene recall ready for {scene_label}");
                        tracing::debug!(event = "scene_recall_start_requested", scene = %scene_label, "Scene recall start requested for {scene_label}");
                        event_bus.publish_scenes(
                            generation,
                            ScenesEvent::Ready {
                                scene_label: scene_label.clone(),
                                target_count: fade_config.targets.len(),
                            },
                        );
                        event_bus.publish_scenes(
                            generation,
                            ScenesEvent::StartRequested {
                                scene_label: scene_label.clone(),
                            },
                        );
                    })
                    .await
                    .is_none()
                {
                    if queued_readiness.is_some() {
                        self.cancel("LV1 connection generation changed", false);
                        self.clear_late_observations();
                    }
                    return;
                }
                let same_scene_behavior = if settings.same_scene_recall_enabled {
                    SameSceneRecallBehavior::FinishActiveTargets
                } else {
                    SameSceneRecallBehavior::OverrideMatchingTargets
                };
                let deadline = readiness.deadline;
                let result = send_fade_checked(fade, lv1, lockout, deadline, |reply| {
                    FadeCommand::RecallSceneFade {
                        config: fade_config,
                        same_scene_behavior,
                        readiness,
                        reply: Some(reply),
                    }
                })
                .await;
                self.finish_fade_handoff(
                    result,
                    queued_readiness,
                    lv1,
                    event_bus,
                    generation,
                    scene_label,
                )
                .await;
            }
            RecallPolicyDecision::Skip { reason } | RecallPolicyDecision::Blocked { reason } => {
                let scene_label = scene_label(&observation.scene);
                if lv1
                    .if_current(|| {
                        let event = if blocked {
                            tracing::warn!(
                                event = "scene_recall_blocked",
                                scene = %scene_label,
                                reason = %reason,
                                "Scene recall blocked for {scene_label}: {reason}"
                            );
                            ScenesEvent::Blocked {
                                scene_label: scene_label.clone(),
                                reason,
                            }
                        } else {
                            ScenesEvent::Skipped {
                                scene_label: scene_label.clone(),
                                reason,
                            }
                        };
                        event_bus.publish_scenes(generation, event);
                    })
                    .await
                    .is_none()
                {
                    if queue_readiness.is_some() {
                        self.cancel("LV1 connection generation changed", false);
                        self.clear_late_observations();
                    }
                    return;
                }

                let Some(queue_readiness) = queue_readiness else {
                    return;
                };
                let Some((readiness, completion)) =
                    self.prepare_readiness(queue_readiness, Instant::now())
                else {
                    self.cancel("LV1 recall readiness was lost", true);
                    return;
                };
                let deadline = readiness.deadline;
                let result = send_fade_checked(fade, lv1, lockout, deadline, |reply| {
                    FadeCommand::WaitForRecallReadiness {
                        scene: FadeSceneIdentity {
                            index: observation.scene.index,
                            name: observation.scene.name,
                        },
                        readiness,
                        reply: Some(reply),
                    }
                })
                .await;
                self.finish_fade_handoff(
                    result,
                    Some((queue_readiness, completion)),
                    lv1,
                    event_bus,
                    generation,
                    scene_label,
                )
                .await;
            }
        }
    }

    async fn finish_fade_handoff(
        &mut self,
        result: Result<(), AppCommandError>,
        queued_readiness: Option<(
            QueueReadiness,
            oneshot::Receiver<Result<(), RecallReadinessError>>,
        )>,
        lv1: &Lv1Connection,
        event_bus: &AppEventBus,
        generation: u64,
        scene_label: String,
    ) {
        match (result, queued_readiness) {
            (Ok(()), Some((readiness, completion))) => {
                if !self.accept_readiness(readiness, completion) {
                    self.cancel("LV1 recall readiness was lost", true);
                }
            }
            (Ok(()), None) => {}
            (Err(AppCommandError::StaleGeneration), Some(_)) => {
                self.cancel("LV1 connection generation changed", false);
                self.clear_late_observations();
            }
            (Err(error), Some(_)) => {
                let current = lv1
                    .if_current(|| {
                        let reason = match &error {
                            AppCommandError::RecallCanceled(reason) => reason.as_str(),
                            _ => "Fade engine is unavailable",
                        };
                        self.cancel(reason, true);
                        event_bus.publish_scenes(
                            generation,
                            ScenesEvent::Blocked {
                                scene_label,
                                reason: format!("failed to start fade: {error:?}"),
                            },
                        );
                    })
                    .await;
                if current.is_none() {
                    self.cancel("LV1 connection generation changed", false);
                    self.clear_late_observations();
                }
            }
            (Err(AppCommandError::StaleGeneration), None) => {}
            (Err(error), None) => {
                lv1.if_current(|| {
                    event_bus.publish_scenes(
                        generation,
                        ScenesEvent::Blocked {
                            scene_label,
                            reason: format!("failed to start fade: {error:?}"),
                        },
                    );
                })
                .await;
            }
        }
    }

    /**
     * @cc [owner:mixxorz,label:product] explicit-recall-admission-reply
     * Explicit recall admission MUST reject invalid or over-capacity requests before enqueueing;
     * an admitted caller reply MUST remain pending until that request is actually dispatched or
     * canceled.
     */
    pub async fn admit_explicit_recall(
        &mut self,
        lockout: &ShowLockoutReader,
        lv1: &Lv1Connection,
        recall_state: &ScenesState,
        internal_scene_id: Uuid,
        reply: oneshot::Sender<Result<RecallSceneResult, AppCommandError>>,
    ) {
        tracing::debug!(
            event = "scene_recall_requested",
            internal_scene_id = %internal_scene_id,
            "Scene recall requested"
        );

        let lv1_snapshot = match explicit_recall_lv1_snapshot(lv1).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                lv1.if_current(|| log_explicit_recall_blocked(internal_scene_id, &error))
                    .await;
                let _ = reply.send(Err(error));
                return;
            }
        };
        let scene_document = recall_state.snapshot();
        if let Err(error) =
            validate_explicit_recall(lockout, &scene_document, &lv1_snapshot, internal_scene_id)
        {
            log_explicit_recall_blocked(internal_scene_id, &error);
            let _ = reply.send(Err(error));
            return;
        }
        if self.is_full() {
            tracing::warn!(
                event = "scene_recall_queue_full",
                internal_scene_id = %internal_scene_id,
                capacity = RECALL_QUEUE_CAPACITY,
                "Scene recall blocked because the recall queue is full"
            );
            let _ = reply.send(Err(AppCommandError::RecallQueueFull));
            return;
        }

        self.waiting.push_back(QueuedRecall {
            request_id: Uuid::new_v4(),
            internal_scene_id,
            reply,
        });
        if self.is_idle() {
            self.dispatch_next(lockout, lv1, recall_state).await;
        }
    }

    /**
     * @cc [owner:mixxorz,label:safety] queued-recall-fresh-dispatch
     * Each FIFO request MUST obtain and validate a fresh connected LV1 snapshot and exact scene
     * identity, then recheck lockout inside the generation-fenced LV1 dispatch. Invalid requests
     * may fail individually, but stale generation, lockout, or dispatch loss MUST cancel later
     * intent.
     */
    pub async fn dispatch_next(
        &mut self,
        lockout: &ShowLockoutReader,
        lv1: &Lv1Connection,
        recall_state: &ScenesState,
    ) {
        let generation = lv1.generation();
        while let Some(queued) = self.take_next() {
            let lv1_snapshot = match explicit_recall_lv1_snapshot(lv1).await {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    let (reason, clear_late) = if error == AppCommandError::StaleGeneration {
                        ("LV1 connection generation changed", true)
                    } else {
                        ("LV1 state is unavailable", false)
                    };
                    let _ = queued
                        .reply
                        .send(Err(AppCommandError::RecallCanceled(reason.to_string())));
                    self.cancel(reason, !clear_late);
                    if clear_late {
                        self.clear_late_observations();
                    }
                    return;
                }
            };
            let scene_document = recall_state.snapshot();
            let result = match validate_explicit_recall(
                lockout,
                &scene_document,
                &lv1_snapshot,
                queued.internal_scene_id,
            ) {
                Ok(result) => result,
                Err(error) => {
                    log_explicit_recall_blocked(queued.internal_scene_id, &error);
                    let _ = queued.reply.send(Err(error));
                    continue;
                }
            };

            let dispatch = lv1
                .request_checked(
                    |reply| Lv1Command::RecallScene {
                        scene_index: result.lv1_scene_index,
                        reply: Some(reply),
                    },
                    || {
                        if lockout.current() {
                            Err(AppCommandError::RecallCanceled(
                                "lockout was enabled".to_string(),
                            ))
                        } else {
                            Ok(())
                        }
                    },
                )
                .await
                .and_then(|result| {
                    result.map_err(|error| match error {
                        Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
                        other => AppCommandError::CommandFailed(other.to_string()),
                    })
                });
            let dispatch = match dispatch {
                Ok(dispatch) => dispatch,
                Err(AppCommandError::StaleGeneration) => {
                    let reason = "LV1 connection generation changed";
                    let _ = queued
                        .reply
                        .send(Err(AppCommandError::RecallCanceled(reason.to_string())));
                    self.cancel(reason, false);
                    self.clear_late_observations();
                    return;
                }
                Err(AppCommandError::RecallCanceled(reason)) if reason == "lockout was enabled" => {
                    let _ = queued
                        .reply
                        .send(Err(AppCommandError::RecallCanceled(reason.clone())));
                    self.cancel(&reason, true);
                    return;
                }
                Err(error) => {
                    log_explicit_recall_blocked(queued.internal_scene_id, &error);
                    let _ = queued.reply.send(Err(error));
                    self.cancel("LV1 recall command is unavailable", true);
                    return;
                }
            };

            tracing::debug!(
                event = "scene_recall_command_sent",
                internal_scene_id = %result.scene.internal_scene_id,
                scene_index = result.scene.scene_index,
                scene_name = %result.scene.scene_name,
                "Scene recall command sent: {}",
                result.scene.scene_name
            );
            self.in_flight = Some(InFlightRecall {
                request_id: queued.request_id,
                generation,
                result: result.clone(),
                phase: InFlightPhase::AwaitingObservation {
                    dispatch_sequence: dispatch.scene_observation_sequence,
                    deadline: Instant::now() + RECALL_READINESS_TIMEOUT,
                },
            });
            let _ = queued.reply.send(Ok(result));
            return;
        }
    }

    fn exact_queue_readiness(
        &self,
        observation: &PendingSceneObservation,
    ) -> Option<QueueReadiness> {
        let in_flight = self.in_flight.as_ref()?;
        let InFlightPhase::AwaitingObservation {
            dispatch_sequence,
            deadline,
        } = in_flight.phase
        else {
            return None;
        };
        (observation.generation == in_flight.generation
            && observation.sequence > dispatch_sequence
            && observation.scene.index == in_flight.result.lv1_scene_index
            && observation.scene.name == in_flight.result.scene.scene_name)
            .then_some(QueueReadiness {
                request_id: in_flight.request_id,
                generation: in_flight.generation,
                deadline,
            })
    }

    /**
     * @cc [owner:mixxorz,label:safety] one-deadline-spans-observation-readiness
     * The Fade readiness request MUST reuse the deadline created at LV1 recall dispatch, so one
     * five-second deadline spans exact-scene observation, Fade admission, and readiness. An
     * already-expired or mismatched request MUST be rejected. The post-readiness interval MUST NOT
     * extend this safety deadline.
     */
    fn prepare_readiness(
        &self,
        readiness: QueueReadiness,
        now: Instant,
    ) -> Option<(
        RecallReadinessRequest,
        oneshot::Receiver<Result<(), RecallReadinessError>>,
    )> {
        if now >= readiness.deadline {
            return None;
        }
        let in_flight = self.in_flight.as_ref()?;
        if in_flight.request_id != readiness.request_id
            || in_flight.generation != readiness.generation
            || !matches!(in_flight.phase, InFlightPhase::AwaitingObservation { .. })
        {
            return None;
        }
        let (completion, completed) = oneshot::channel();
        Some((
            RecallReadinessRequest {
                deadline: readiness.deadline,
                completion: Some(completion),
            },
            completed,
        ))
    }

    fn accept_readiness(
        &mut self,
        readiness: QueueReadiness,
        completion: oneshot::Receiver<Result<(), RecallReadinessError>>,
    ) -> bool {
        let Some(in_flight) = self.in_flight.as_mut() else {
            return false;
        };
        if in_flight.request_id != readiness.request_id
            || in_flight.generation != readiness.generation
            || !matches!(in_flight.phase, InFlightPhase::AwaitingObservation { .. })
        {
            return false;
        }
        in_flight.phase = InFlightPhase::AwaitingReadiness {
            completion: Some(completion),
        };
        true
    }
}

async fn send_fade_checked(
    fade: &FadeEngineHandle,
    lv1: &Lv1Connection,
    lockout: &ShowLockoutReader,
    deadline: Instant,
    build_command: impl FnOnce(oneshot::Sender<Result<(), AppCommandError>>) -> FadeCommand,
) -> Result<(), AppCommandError> {
    if lockout.current() {
        return Err(AppCommandError::RecallCanceled(
            "lockout was enabled".to_string(),
        ));
    }
    if Instant::now() >= deadline {
        return Err(AppCommandError::RecallCanceled(
            "LV1 recall readiness was lost".to_string(),
        ));
    }

    let mut lockout_changes = lockout.clone();
    let permit = loop {
        tokio::select! {
            changed = lockout_changes.changed() => match changed {
                Ok(true) => return Err(AppCommandError::RecallCanceled("lockout was enabled".to_string())),
                Ok(false) => continue,
                Err(_) => return Err(AppCommandError::RecallCanceled("lockout state is unavailable".to_string())),
            },
            _ = tokio::time::sleep_until(deadline) => {
                return Err(AppCommandError::RecallCanceled("LV1 recall readiness was lost".to_string()));
            }
            permit = fade.reserve() => {
                break permit.map_err(|_| AppCommandError::FadeUnavailable)?;
            }
        }
    };

    let (reply, response) = oneshot::channel();
    lv1.if_current(|| {
        if lockout.current() {
            return Err(AppCommandError::RecallCanceled(
                "lockout was enabled".to_string(),
            ));
        }
        if Instant::now() >= deadline {
            return Err(AppCommandError::RecallCanceled(
                "LV1 recall readiness was lost".to_string(),
            ));
        }
        permit.send(build_command(reply));
        Ok(())
    })
    .await
    .ok_or(AppCommandError::StaleGeneration)??;

    let result = tokio::select! {
        response = response => response.map_err(|_| AppCommandError::ReplyChannelClosed)?,
        _ = tokio::time::sleep_until(deadline) => {
            return Err(AppCommandError::RecallCanceled(
                "LV1 recall readiness was lost".to_string(),
            ));
        }
    };
    if Instant::now() >= deadline {
        return Err(AppCommandError::RecallCanceled(
            "LV1 recall readiness was lost".to_string(),
        ));
    }
    lv1.ensure_current().await?;
    result
}

async fn explicit_recall_lv1_snapshot(
    lv1: &Lv1Connection,
) -> Result<Lv1StateSnapshot, AppCommandError> {
    lv1.request(|reply| Lv1Command::GetState { reply })
        .await
        .map_err(|error| match error {
            AppCommandError::Lv1Unavailable => AppCommandError::CommandFailed(
                "Recall blocked: LV1 state is unavailable".to_string(),
            ),
            other => other,
        })
}

fn validate_explicit_recall(
    lockout: &ShowLockoutReader,
    scene_document: &SceneDocument,
    lv1_snapshot: &Lv1StateSnapshot,
    internal_scene_id: Uuid,
) -> Result<RecallSceneResult, AppCommandError> {
    crate::scenes::validate_recall_scene_request(
        lockout.current(),
        scene_document,
        lv1_snapshot,
        internal_scene_id,
    )
    .map_err(AppCommandError::CommandFailed)
}

fn log_explicit_recall_blocked(internal_scene_id: Uuid, error: &AppCommandError) {
    tracing::warn!(
        event = "scene_recall_blocked",
        internal_scene_id = %internal_scene_id,
        reason = %error,
        "Scene recall blocked: {error}"
    );
}

fn scene_label(scene: &SceneState) -> String {
    format!("{}: {}", scene.index, scene.name)
}

/**
 * @cc [owner:mixxorz,label:safety] fresh-scene-snapshot-exactness
 * Fresh-state acquisition MUST return only a connected snapshot whose current scene exactly
 * matches both the requested index and name. Mismatch or disconnection MUST retry for at most two
 * seconds, clamped to an existing queued-recall safety deadline when supplied, before returning a
 * timeout error; an LV1 state-request error MUST return immediately rather than continue retrying.
 */
async fn fresh_lv1_snapshot(
    lv1: &Lv1Connection,
    scene: &SceneState,
    safety_deadline: Option<Instant>,
) -> Result<Lv1StateSnapshot, AppCommandError> {
    let fresh_state_deadline = Instant::now() + Duration::from_secs(2);
    let deadline = safety_deadline
        .map(|safety_deadline| safety_deadline.min(fresh_state_deadline))
        .unwrap_or(fresh_state_deadline);
    loop {
        let snapshot = tokio::time::timeout_at(
            deadline,
            lv1.request(|reply| Lv1Command::GetState { reply }),
        )
        .await
        .map_err(|_| fresh_scene_timeout(scene))??;
        if snapshot.connection == ConnectionStatus::Connected
            && snapshot.scene.as_ref() == Some(scene)
        {
            return Ok(snapshot);
        }
        if Instant::now() >= deadline {
            return Err(fresh_scene_timeout(scene));
        }
        tokio::time::sleep_until((Instant::now() + Duration::from_millis(10)).min(deadline)).await;
    }
}

fn fresh_scene_timeout(scene: &SceneState) -> AppCommandError {
    AppCommandError::CommandFailed(format!(
        "timed out waiting for fresh LV1 scene to match recalled scene {}: {}",
        scene.index, scene.name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenes::{SceneConfig, SceneScopeToggles};

    fn in_flight(request_id: Uuid, phase: InFlightPhase) -> InFlightRecall {
        InFlightRecall {
            request_id,
            generation: 1,
            result: RecallSceneResult {
                scene: SceneConfig {
                    internal_scene_id: Uuid::new_v4(),
                    scene_index: Some(1),
                    scene_name: "Intro".to_string(),
                    duration_ms: 1_000,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: SceneScopeToggles::default(),
                },
                lv1_scene_index: 1,
            },
            phase,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn fresh_scene_snapshot_bounds_a_stalled_state_request() {
        let authority = crate::runtime::generation::RuntimeGeneration::new();
        authority.set(1).await;
        let (tx, mut commands) = tokio::sync::mpsc::channel(1);
        let connection = Lv1Connection::new(crate::lv1::test_actor_handle(tx), authority, 1);
        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        let request =
            tokio::spawn(async move { fresh_lv1_snapshot(&connection, &scene, None).await });
        let _stalled_request = commands.recv().await.expect("expected state request");

        tokio::time::advance(Duration::from_secs(2) + Duration::from_millis(1)).await;
        tokio::task::yield_now().await;

        assert!(
            request.is_finished(),
            "state request exceeded its two-second bound"
        );
        assert!(matches!(
            request.await.unwrap(),
            Err(AppCommandError::CommandFailed(message))
                if message == "timed out waiting for fresh LV1 scene to match recalled scene 1: Intro"
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn fresh_scene_snapshot_respects_existing_safety_deadline() {
        let authority = crate::runtime::generation::RuntimeGeneration::new();
        authority.set(1).await;
        let (tx, mut commands) = tokio::sync::mpsc::channel(1);
        let connection = Lv1Connection::new(crate::lv1::test_actor_handle(tx), authority, 1);
        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        let safety_deadline = Instant::now() + Duration::from_millis(100);
        let request = tokio::spawn(async move {
            fresh_lv1_snapshot(&connection, &scene, Some(safety_deadline)).await
        });
        let _stalled_request = commands.recv().await.expect("expected state request");

        tokio::time::advance(Duration::from_millis(101)).await;
        tokio::task::yield_now().await;

        assert!(
            request.is_finished(),
            "state request exceeded the queued recall safety deadline"
        );
        assert!(matches!(
            request.await.unwrap(),
            Err(AppCommandError::CommandFailed(message))
                if message == "timed out waiting for fresh LV1 scene to match recalled scene 1: Intro"
        ));
    }

    #[test]
    fn early_interval_deadline_has_no_effect() {
        let now = Instant::now();
        let until = now + Duration::from_secs(1);
        let mut coordinator = RecallCoordinator {
            in_flight: Some(in_flight(
                Uuid::new_v4(),
                InFlightPhase::PostReadinessInterval { until },
            )),
            ..Default::default()
        };
        let deadline = coordinator.deadline().unwrap();

        assert!(!coordinator.handle_deadline(deadline, now));
        assert!(coordinator.deadline().is_some());
        assert!(coordinator.handle_deadline(deadline, until));
        assert!(coordinator.deadline().is_none());
    }

    #[test]
    fn stale_deadline_cannot_change_a_new_request_or_phase() {
        let now = Instant::now();
        let old_request = Uuid::new_v4();
        let mut coordinator = RecallCoordinator {
            in_flight: Some(in_flight(
                old_request,
                InFlightPhase::AwaitingObservation {
                    dispatch_sequence: 10,
                    deadline: now + Duration::from_secs(1),
                },
            )),
            ..Default::default()
        };
        let stale_deadline = coordinator.deadline().unwrap();
        let new_request = Uuid::new_v4();
        coordinator.in_flight = Some(in_flight(
            new_request,
            InFlightPhase::PostReadinessInterval {
                until: now + Duration::from_secs(2),
            },
        ));

        assert!(!coordinator.handle_deadline(stale_deadline, now + Duration::from_secs(3)));
        assert_eq!(coordinator.in_flight.unwrap().request_id, new_request);
    }
}

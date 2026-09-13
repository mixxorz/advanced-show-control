use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::sync::{mpsc, oneshot};

use crate::fade::{FadeCommand, FadeEngineHandle};
use crate::lv1::{
    ConnectionStatus, Lv1ActorError, Lv1ActorHandle, Lv1Command, Lv1Connection, Lv1Event,
    Lv1StateSnapshot, RecallSceneDispatch, SceneObservation,
};
use crate::runtime::errors::AppCommandError;
use crate::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use crate::runtime::generation::RuntimeGeneration;
use crate::scenes::ScenesHandle;
#[cfg(test)]
use crate::scenes::recall_coordinator::{
    BeforeFadeHandoff, LATE_CANCELED_OBSERVATION_CAPACITY, LATE_CANCELED_OBSERVATION_TTL,
    RECALL_READINESS_TIMEOUT,
};
use crate::scenes::recall_coordinator::{
    ObservationFadeRequest, ObservationHandoffCompletion, PendingRecallAdmission,
    PendingRecallDispatch, PendingSceneObservation, PreparedObservationHandoff,
    PreparedRecallDispatch, PreparedSceneObservation, RecallCoordinator, RecallReadinessOutcome,
    fresh_lv1_snapshot, send_fade_checked,
};
use crate::scenes::scene_alignment::scene_alignment_diagnostic;
use crate::scenes::{
    ScenesCommand, ScenesCommandResult, ScenesEvent, ScenesState, SelectedSceneResult,
};
use crate::settings::{AppSettings, SettingsCommand, SettingsEvent, SettingsHandle};
use crate::show::ShowLockoutReader;

#[derive(Clone)]
pub struct ScenesPeers {
    authority: RuntimeGeneration,
    peers: Arc<Mutex<Option<ScenesPeerHandles>>>,
}

#[derive(Clone)]
struct ScenesPeerHandles {
    lv1: Lv1Connection,
    fade: FadeEngineHandle,
}

impl ScenesPeers {
    pub fn set_peers_for_generation(
        &self,
        generation: u64,
        lv1: Lv1ActorHandle,
        fade: FadeEngineHandle,
    ) {
        *self.peers.lock().expect("scene recall peer lock poisoned") = Some(ScenesPeerHandles {
            lv1: Lv1Connection::new(lv1, self.authority.clone(), generation),
            fade,
        });
    }

    pub fn clear_peers_for_generation(&self, generation: u64) {
        let mut peers = self.peers.lock().expect("scene recall peer lock poisoned");
        if peers
            .as_ref()
            .is_some_and(|current| current.lv1.generation() == generation)
        {
            *peers = None;
        }
    }

    /**
     * @cc [owner:mixxorz,label:safety] exact-generation-peer-access
     * Connection-dependent scene work MUST receive peers only when the installed LV1 connection
     * is bound to the requested generation; peers from any other generation are unavailable.
     */
    fn handles(&self, generation: u64) -> Option<ScenesPeerHandles> {
        self.peers
            .lock()
            .expect("scene recall peer lock poisoned")
            .as_ref()
            .filter(|peers| peers.lv1.generation() == generation)
            .cloned()
    }
}

pub struct ScenesTask {
    initial_generation: u64,
    runtime_generation: RuntimeGeneration,
    peers: ScenesPeers,
    event_bus: AppEventBus,
    events: tokio::sync::broadcast::Receiver<AppEvent>,
    settings_handle: SettingsHandle,
    initial_settings: AppSettings,
    lockout: ShowLockoutReader,
    command_rx: mpsc::Receiver<ScenesCommand>,
    cue_lists: crate::cue_lists::CueListsHandle,
    cue_commands: mpsc::Receiver<crate::cue_lists::CueListsCommand>,
    #[cfg(test)]
    pending_scene_observer: Option<oneshot::Sender<()>>,
    #[cfg(test)]
    before_fade_handoff: Option<BeforeFadeHandoff>,
}

impl ScenesTask {
    pub fn cue_lists_handle(&self) -> crate::cue_lists::CueListsHandle {
        self.cue_lists.clone()
    }

    pub fn spawn(self) {
        tokio::spawn(run_scenes_actor(self));
    }
}

pub fn build_scenes_actor(
    generation: u64,
    runtime_generation: RuntimeGeneration,
    event_bus: AppEventBus,
    events: tokio::sync::broadcast::Receiver<AppEvent>,
    settings_handle: SettingsHandle,
    initial_settings: AppSettings,
    lockout: ShowLockoutReader,
) -> (ScenesHandle, ScenesTask, ScenesPeers) {
    let (command_tx, command_rx) = mpsc::channel(8);
    let (cue_lists, cue_commands) = mpsc::channel(8);

    let handle = command_tx;
    let peers = ScenesPeers {
        authority: runtime_generation.clone(),
        peers: Arc::default(),
    };
    let task = ScenesTask {
        initial_generation: generation,
        runtime_generation,
        peers: peers.clone(),
        events,
        event_bus,
        settings_handle,
        initial_settings,
        lockout,
        command_rx,
        cue_lists,
        cue_commands,
        #[cfg(test)]
        pending_scene_observer: None,
        #[cfg(test)]
        before_fade_handoff: None,
    };
    (handle, task, peers)
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn build_scenes_actor_with_pending_scene_observer(
    generation: u64,
    runtime_generation: RuntimeGeneration,
    event_bus: AppEventBus,
    events: tokio::sync::broadcast::Receiver<AppEvent>,
    settings_handle: SettingsHandle,
    initial_settings: AppSettings,
    lockout: ShowLockoutReader,
    pending_scene_observer: oneshot::Sender<()>,
) -> (ScenesHandle, ScenesTask, ScenesPeers) {
    let (handle, mut task, peers) = build_scenes_actor(
        generation,
        runtime_generation,
        event_bus,
        events,
        settings_handle,
        initial_settings,
        lockout,
    );
    task.pending_scene_observer = Some(pending_scene_observer);
    (handle, task, peers)
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn build_scenes_actor_with_before_fade_handoff(
    generation: u64,
    runtime_generation: RuntimeGeneration,
    event_bus: AppEventBus,
    events: tokio::sync::broadcast::Receiver<AppEvent>,
    settings_handle: SettingsHandle,
    initial_settings: AppSettings,
    lockout: ShowLockoutReader,
    before_fade_handoff: BeforeFadeHandoff,
) -> (ScenesHandle, ScenesTask, ScenesPeers) {
    let (handle, mut task, peers) = build_scenes_actor(
        generation,
        runtime_generation,
        event_bus,
        events,
        settings_handle,
        initial_settings,
        lockout,
    );
    task.before_fade_handoff = Some(before_fade_handoff);
    (handle, task, peers)
}

/**
 * @cc [owner:mixxorz,label:safety] event-bus-lag-fails-closed
 * On event-bus lag, the actor MUST synchronously drain queued facts, reconcile the active
 * generation, cancel queued recall intent without aborting Fade, and clear pending observations
 * and runtime readiness. Settings refresh and the optional fresh connected LV1 snapshot MUST run
 * as one actor-owned pending operation polled by the main select. Generation change, disconnect,
 * or newer lag MUST cancel stale recovery, and completion MUST NOT overwrite Settings or scene-list
 * facts observed after recovery began. If Settings cannot be refreshed, recall automation MUST
 * remain stopped rather than continue with stale policy.
 */
/**
 * @cc [owner:mixxorz,label:safety] scene-facts-precede-pending-commits
 * Pending-operation completions and settled observations MUST NOT commit while runtime, LV1, or
 * Settings facts are already queued. Selection MUST remain fair so fact traffic cannot starve
 * commands, lockout changes, or recall deadlines; derived-state completion MAY wait for the fact
 * backlog to clear and MUST fail closed if that backlog causes receiver lag.
 */
async fn run_scenes_actor(task: ScenesTask) {
    let ScenesTask {
        initial_generation,
        runtime_generation,
        peers,
        event_bus,
        mut events,
        settings_handle,
        initial_settings,
        mut lockout,
        mut command_rx,
        cue_lists,
        mut cue_commands,
        #[cfg(test)]
        mut pending_scene_observer,
        #[cfg(test)]
        mut before_fade_handoff,
    } = task;

    drop(cue_lists);
    let mut active_generation = initial_generation;
    let mut cached_scene_list: Option<Vec<crate::lv1::SceneListEntry>> = None;
    let mut scene_library_status = SceneLibraryStatus::AwaitingPeers;
    let mut recall_state = ScenesState::default();
    let mut cues = crate::cue_lists::operations::CueLists::new(event_bus.clone());
    let mut cue_commands_open = true;
    let mut scene_commands_open = true;
    let mut held_scene_command: Option<ScenesCommand> = None;
    let mut recall_coordinator = RecallCoordinator::default();
    let mut settings = initial_settings;
    let mut lockout_open = true;
    let mut pending_snapshot: Option<PendingSnapshotOperation> = None;
    let mut pending_lag_recovery: Option<PendingLagRecovery> = None;
    let mut pending_abort: Option<PendingAbortOperation> = None;
    let mut pending_recall: Option<PendingExplicitRecallOperation> = None;
    let mut pending_observation_settings: Option<PendingObservationSettings> = None;
    let mut pending_observation_snapshot: Option<PendingObservationSnapshot> = None;
    let mut pending_observation_handoff: Option<PendingObservationHandoff> = None;
    let mut settings_revision = 0_u64;
    let mut scene_list_revision = 0_u64;

    // Recall timing windows:
    //
    // - 25 ms settle:         Allows LV1 scene-state to stabilize after a scene change event.
    //                         The scene name/index can arrive in multiple frames; we wait for
    //                         the dust to settle before evaluating recall policy.
    //
    // - 500 ms edit suppression: After the scene list is modified, suppress recall to avoid
    //                         triggering fades against a partially-edited session.
    //
    // - 2 s arming delay:     The first scene seen after arming is treated as the baseline
    //                         (current scene at arm time), not a scene change to recall.
    //
    // - Configurable repeat delay (500 ms default): Prevents the same scene from triggering two
    //                         consecutive recalls if a bounce or duplicate event arrives.
    loop {
        if !scene_commands_open && !cue_commands_open {
            break;
        }
        let scene_ids: Vec<_> = recall_state
            .scene_configs()
            .iter()
            .map(|scene| scene.internal_scene_id)
            .collect();
        let recall_deadline = recall_coordinator.deadline();
        let recall_timer = async move {
            match recall_deadline {
                Some(deadline) => tokio::time::sleep_until(deadline.at()).await,
                None => std::future::pending::<()>().await,
            }
        };
        let pending_scene_deadline = recall_coordinator.pending_observation_deadline();
        let pending_scene_settle = async move {
            match pending_scene_deadline {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            completion = recall_coordinator.readiness_completion(), if events.is_empty() && events.sender_strong_count() > 0 => {
                if runtime_generation.current().await != completion.generation() {
                    cancel_pending_explicit_recall(
                        pending_recall.take(),
                        &mut recall_coordinator,
                        "LV1 connection generation changed",
                    );
                    recall_coordinator.cancel("LV1 connection generation changed", false);
                    recall_coordinator.clear_late_observations();
                    continue;
                }
                match recall_coordinator.handle_readiness_completion(
                    completion,
                    Duration::from_millis(settings.asc_recall_interval_ms.min(10_000)),
                    tokio::time::Instant::now(),
                ) {
                    RecallReadinessOutcome::Failed => {
                        cancel_pending_explicit_recall(
                            pending_recall.take(),
                            &mut recall_coordinator,
                            "LV1 recall readiness was lost",
                        );
                    }
                    RecallReadinessOutcome::ReadyToDispatch if pending_recall.is_none() => {
                        pending_recall = start_next_recall_dispatch(
                            &mut recall_coordinator,
                            &peers,
                            active_generation,
                        );
                        if pending_recall.is_none() && peers.handles(active_generation).is_none() {
                            recall_coordinator.cancel("LV1 state is unavailable", true);
                        }
                    }
                    RecallReadinessOutcome::Ignored
                    | RecallReadinessOutcome::IntervalStarted
                    | RecallReadinessOutcome::ReadyToDispatch => {}
                }
            }
            event = events.recv() => {
                match event {
                    Ok(AppEvent::Lv1 { generation: event_generation, event: Lv1Event::SceneListChanged(scene_list) }) if event_generation == active_generation => {
                        scene_list_revision = scene_list_revision.wrapping_add(1);
                        let cached_before_ready = runtime_generation
                            .if_current(event_generation, || {
                                match scene_library_status {
                                    SceneLibraryStatus::AwaitingPeers => {
                                        cached_scene_list = Some(scene_list);
                                        true
                                    }
                                    SceneLibraryStatus::AwaitingSceneList => {
                                        scene_library_status = SceneLibraryStatus::Ready;
                                        apply_scene_list(
                                            &mut recall_state,
                                            &event_bus,
                                            active_generation,
                                            scene_list,
                                        );
                                        false
                                    }
                                    SceneLibraryStatus::Ready => {
                                        apply_scene_list(
                                            &mut recall_state,
                                            &event_bus,
                                            active_generation,
                                            scene_list,
                                        );
                                        false
                                    }
                                }
                            })
                            .await
                            .unwrap_or(false);
                        if cached_before_ready {
                            continue;
                        }
                    }
                    Ok(AppEvent::Lv1 { generation: event_generation, event: Lv1Event::SceneChanged(SceneObservation { sequence, scene }) }) if scene_library_status == SceneLibraryStatus::Ready && accepts_scene_observation_generation(event_generation, active_generation) => {
                        recall_coordinator.observe_scene(event_generation, sequence, scene, tokio::time::Instant::now());
                        #[cfg(test)]
                        if let Some(observer) = pending_scene_observer.take() {
                            let _ = observer.send(());
                        }
                    }
                    Ok(AppEvent::Settings(SettingsEvent::StateChanged { settings: updated_settings })) => {
                        settings_revision = settings_revision.wrapping_add(1);
                        settings = updated_settings;
                    }
                    Ok(AppEvent::Lv1 { generation: event_generation, event: Lv1Event::Disconnected { .. } }) if event_generation == active_generation => {
                        pending_lag_recovery = None;
                        pending_observation_settings = None;
                        pending_observation_snapshot = None;
                        pending_observation_handoff = None;
                        cancel_pending_explicit_recall(
                            pending_recall.take(),
                            &mut recall_coordinator,
                            "LV1 disconnected",
                        );
                        if let Some(operation) = pending_abort.take() {
                            operation.cancel(AppCommandError::FadeUnavailable);
                        }
                        scene_list_revision = scene_list_revision.wrapping_add(1);
                        if let Some(operation) = pending_snapshot.take() {
                            operation.cancel("Store scene blocked: LV1 disconnected");
                        }
                        cached_scene_list = None;
                        scene_library_status = SceneLibraryStatus::AwaitingSceneList;
                        recall_state.mark_scene_library_unavailable();
                        recall_coordinator.clear_pending_observation();
                        recall_coordinator.cancel("LV1 disconnected", true);
                        recall_coordinator.clear_late_observations();
                        publish_scene_state_changed(
                            &event_bus,
                            active_generation,
                            &recall_state,
                            false,
                        );
                    }
                    Ok(AppEvent::Runtime(crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged { generation: event_generation })) if event_generation != active_generation => {
                        pending_lag_recovery = None;
                        pending_observation_settings = None;
                        pending_observation_snapshot = None;
                        pending_observation_handoff = None;
                        cancel_pending_explicit_recall(
                            pending_recall.take(),
                            &mut recall_coordinator,
                            "LV1 connection generation changed",
                        );
                        if let Some(operation) = pending_abort.take() {
                            operation.cancel(AppCommandError::StaleGeneration);
                        }
                        scene_list_revision = scene_list_revision.wrapping_add(1);
                        if let Some(operation) = pending_snapshot.take() {
                            operation.cancel("Store scene blocked: LV1 generation changed");
                        }
                        transition_scene_generation(
                            &mut active_generation,
                            event_generation,
                            &mut cached_scene_list,
                            &mut scene_library_status,
                            &mut recall_state,
                            &mut recall_coordinator,
                            &peers,
                            &event_bus,
                        );
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        log_lagged_subscriber("scene-recall", count);
                        pending_observation_settings = None;
                        pending_observation_snapshot = None;
                        pending_observation_handoff = None;
                        cancel_pending_explicit_recall(
                            pending_recall.take(),
                            &mut recall_coordinator,
                            "LV1 recall readiness was lost",
                        );
                        drain_retained_events(&mut events);
                        let current_generation = runtime_generation.current().await;
                        if current_generation != active_generation {
                            if let Some(operation) = pending_abort.take() {
                                operation.cancel(AppCommandError::StaleGeneration);
                            }
                            scene_list_revision = scene_list_revision.wrapping_add(1);
                            transition_scene_generation(
                                &mut active_generation,
                                current_generation,
                                &mut cached_scene_list,
                                &mut scene_library_status,
                                &mut recall_state,
                                &mut recall_coordinator,
                                &peers,
                                &event_bus,
                            );
                        }
                        recall_coordinator.cancel("LV1 recall readiness was lost", true);
                        recall_coordinator.clear_pending_observation();
                        scene_library_status = SceneLibraryStatus::AwaitingSceneList;
                        recall_state.mark_scene_library_unavailable();
                        publish_scene_state_changed(
                            &event_bus,
                            active_generation,
                            &recall_state,
                            false,
                        );
                        pending_lag_recovery = Some(PendingLagRecovery::new(
                            active_generation,
                            settings_revision,
                            scene_list_revision,
                            settings_handle.clone(),
                            peers.handles(active_generation).map(|handles| handles.lv1),
                        ));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        recall_coordinator.cancel("scene recall event stream closed", true);
                        recall_coordinator.clear_late_observations();
                        break;
                    }
                }
            }
            command = cue_commands.recv(), if pending_snapshot.is_none() && pending_abort.is_none() && pending_recall.is_none() && pending_observation_settings.is_none() && pending_observation_snapshot.is_none() && pending_observation_handoff.is_none() && cue_commands_open && !cues.recall_pending() => {
                match command {
                    Some(crate::cue_lists::CueListsCommand::RecallCuedCue { reply }) => {
                        if let Some(command) = cues.begin_recall(reply) {
                            let ScenesCommand::RecallScene { internal_scene_id, reply } = command else {
                                unreachable!("cue recall must enter the scene recall queue");
                            };
                            if scene_library_status != SceneLibraryStatus::Ready {
                                let _ = reply.send(Err(AppCommandError::ScenesUnavailable));
                                continue;
                            }
                            let Some(peer_handles) = peers.handles(active_generation) else {
                                let _ = reply.send(Err(AppCommandError::ScenesUnavailable));
                                continue;
                            };
                            let admission = recall_coordinator.begin_admission(internal_scene_id, reply);
                            pending_recall = Some(PendingExplicitRecallOperation::snapshot_for_admission(
                                peer_handles.lv1,
                                admission,
                            ));
                        }
                    }
                    Some(crate::cue_lists::CueListsCommand::Shutdown) | None => cue_commands_open = false,
                    Some(command) => cues.dispatch(command),
                }
            }
            () = cues.complete_recall(), if events.is_empty() && events.sender_strong_count() > 0 => {}
            command = async {
                match held_scene_command.take() {
                    Some(command) => Some(command),
                    None => command_rx.recv().await,
                }
            }, if scene_commands_open && ((pending_snapshot.is_none() && pending_abort.is_none() && pending_recall.is_none() && pending_observation_settings.is_none() && pending_observation_snapshot.is_none() && pending_observation_handoff.is_none()) || held_scene_command.is_none()) => {
                let Some(command) = command else {
                    scene_commands_open = false;
                    continue;
                };
                if pending_snapshot.is_some() || pending_abort.is_some() || pending_recall.is_some() || pending_observation_settings.is_some() || pending_observation_snapshot.is_some() || pending_observation_handoff.is_some() {
                    if matches!(command, ScenesCommand::Shutdown) {
                        if let Some(operation) = pending_snapshot.take() {
                            operation.cancel("Store scene blocked: Scenes actor stopped");
                        }
                        cancel_pending_explicit_recall(
                            pending_recall.take(),
                            &mut recall_coordinator,
                            "Scenes actor stopped",
                        );
                        break;
                    }
                    if pending_abort.is_none()
                        && matches!(&command, ScenesCommand::AbortAll { .. })
                    {
                        let ScenesCommand::AbortAll { reply } = command else {
                            unreachable!("matched Abort All command");
                        };
                        if let Some(operation) = pending_snapshot.take() {
                            operation.cancel("Store scene canceled: Abort All was requested");
                        }
                        cancel_pending_explicit_recall(
                            pending_recall.take(),
                            &mut recall_coordinator,
                            "Abort All was requested",
                        );
                        pending_observation_settings = None;
                        pending_observation_snapshot = None;
                        pending_observation_handoff = None;
                        recall_coordinator.cancel("Abort All was requested", true);
                        pending_abort = Some(PendingAbortOperation::new(
                            active_generation,
                            peers.handles(active_generation).map(|handles| {
                                (handles.lv1, handles.fade)
                            }),
                            reply,
                        ));
                        continue;
                    }
                    if !matches!(command, ScenesCommand::ReplaceSessionDocument { .. })
                        || (pending_snapshot.is_none() && pending_recall.is_none() && pending_observation_settings.is_none() && pending_observation_snapshot.is_none() && pending_observation_handoff.is_none())
                    {
                        held_scene_command = Some(command);
                        continue;
                    }
                    if let Some(operation) = pending_snapshot.take() {
                        operation.cancel("Store scene canceled: session was replaced");
                    }
                    cancel_pending_explicit_recall(
                        pending_recall.take(),
                        &mut recall_coordinator,
                        "session was replaced",
                    );
                    pending_observation_settings = None;
                    pending_observation_snapshot = None;
                    pending_observation_handoff = None;
                }
                let command = match command {
                    ScenesCommand::AbortAll { reply } => {
                        recall_coordinator.cancel("Abort All was requested", true);
                        pending_abort = Some(PendingAbortOperation::new(
                            active_generation,
                            peers.handles(active_generation).map(|handles| {
                                (handles.lv1, handles.fade)
                            }),
                            reply,
                        ));
                        continue;
                    }
                    ScenesCommand::GetSessionDocument { reply } => {
                        let _ = reply.send(crate::session::SessionDocument {
                            scenes: recall_state.snapshot(), cue_lists: cues.state.document(),
                        });
                        continue;
                    }
                    ScenesCommand::ReplaceSessionDocument { replacement, expected_generation, reply } => {
                        let result = runtime_generation.if_current(expected_generation, || {
                            replacement.commit(|document| {
                                recall_coordinator.cancel("session was replaced", true);
                                cues.cancel_recall();
                                recall_state.replace_snapshot_for_session(document.scenes);
                                cues.state.replace_document(document.cue_lists, recall_state.scene_configs().iter().map(|scene| scene.internal_scene_id));
                                event_bus.publish(AppEvent::SessionReplaced {
                                    generation: active_generation, scenes: recall_state.projection_state(),
                                    cue_lists: crate::cue_lists::CueListsProjectionState { document: cues.state.document(), last_recall_status: None },
                                });
                                crate::session::SessionDocument { scenes: recall_state.snapshot(), cue_lists: cues.state.document() }
                            })
                        }).await.unwrap_or_else(|| Err("LV1 generation is no longer current".into()));
                        let _ = reply.send(result);
                        continue;
                    }
                    ScenesCommand::RecallScene { internal_scene_id, reply } => {
                        if scene_library_status != SceneLibraryStatus::Ready {
                            let _ = reply.send(Err(AppCommandError::ScenesUnavailable));
                            continue;
                        }
                        let Some(peer_handles) = peers.handles(active_generation) else {
                            let _ = reply.send(Err(AppCommandError::ScenesUnavailable));
                            continue;
                        };
                        let admission = recall_coordinator.begin_admission(internal_scene_id, reply);
                        pending_recall = Some(PendingExplicitRecallOperation::snapshot_for_admission(
                            peer_handles.lv1,
                            admission,
                        ));
                        continue;
                    }
                    ScenesCommand::StoreSceneConfigFromCurrentLv1 { internal_scene_id, reply } => {
                        if scene_library_status != SceneLibraryStatus::Ready {
                            if let Some(reply) = reply {
                                let _ = reply.send(Err(AppCommandError::ScenesUnavailable.to_string()));
                            }
                            continue;
                        }
                        let Some(peer_handles) = peers.handles(active_generation) else {
                            if let Some(reply) = reply {
                                let _ = reply.send(Err(AppCommandError::Lv1Unavailable.to_string()));
                            }
                            continue;
                        };
                        pending_snapshot = Some(PendingSnapshotOperation::store(
                            peer_handles.lv1,
                            internal_scene_id,
                            reply,
                        ));
                        continue;
                    }
                    command => command,
                };
                if matches!(&command, ScenesCommand::RuntimePeersReady { .. }) {
                    let authoritative_generation = runtime_generation.current().await;
                    if authoritative_generation != active_generation {
                        transition_scene_generation(
                            &mut active_generation,
                            authoritative_generation,
                            &mut cached_scene_list,
                            &mut scene_library_status,
                            &mut recall_state,
                            &mut recall_coordinator,
                            &peers,
                            &event_bus,
                        );
                    }
                }
                if dispatch_scenes_command(
                    command,
                    &mut recall_state,
                    &mut recall_coordinator,
                    &peers,
                    &event_bus,
                    active_generation,
                    &runtime_generation,
                    &lockout,
                    &mut cached_scene_list,
                    &mut scene_library_status,
                )
                .await
                    == ScenesCommandDispatch::Shutdown
                {
                    break;
                }
            }
            recovered_settings = async {
                pending_observation_settings
                    .as_mut()
                    .expect("enabled observation-settings branch has an operation")
                    .future
                    .as_mut()
                    .await
            }, if pending_observation_settings.is_some() && events.is_empty() && events.sender_strong_count() > 0 => {
                let operation = pending_observation_settings
                    .take()
                    .expect("completed observation-settings operation exists");
                let Some(recovered_settings) = recovered_settings else {
                    break;
                };
                if operation.generation != active_generation
                    || runtime_generation.current().await != operation.generation
                    || scene_library_status != SceneLibraryStatus::Ready
                {
                    continue;
                }
                if settings_revision == operation.settings_revision {
                    settings = recovered_settings;
                }
                let Some(prepared) = recall_coordinator.prepare_scene_observation(
                    &mut recall_state,
                    &settings,
                    operation.observation,
                ) else {
                    continue;
                };
                let Some(peer_handles) = peers.handles(active_generation) else {
                    continue;
                };
                pending_observation_snapshot = Some(PendingObservationSnapshot::new(
                    peer_handles.lv1,
                    prepared,
                ));
            }
            completion = async {
                pending_observation_snapshot
                    .as_mut()
                    .expect("enabled observation-snapshot branch has an operation")
                    .future
                    .as_mut()
                    .await
            }, if pending_observation_snapshot.is_some() && events.is_empty() && events.sender_strong_count() > 0 => {
                let operation = pending_observation_snapshot
                    .take()
                    .expect("completed observation-snapshot operation exists");
                if operation.generation != active_generation
                    || runtime_generation.current().await != operation.generation
                    || scene_library_status != SceneLibraryStatus::Ready
                {
                    continue;
                }
                let Some(peer_handles) = peers.handles(active_generation) else {
                    continue;
                };
                let Some(prepared) = recall_coordinator.prepare_observation_handoff(
                    &event_bus,
                    &mut recall_state,
                    &settings,
                    &lockout,
                    operation.prepared,
                    completion,
                ) else {
                    continue;
                };
                pending_observation_handoff = Some(PendingObservationHandoff::new(
                    peer_handles.lv1,
                    peer_handles.fade,
                    event_bus.clone(),
                    lockout.clone(),
                    #[cfg(test)]
                    before_fade_handoff.take(),
                    prepared,
                ));
            }
            completion = async {
                pending_observation_handoff
                    .as_mut()
                    .expect("enabled observation-handoff branch has an operation")
                    .future
                    .as_mut()
                    .await
            }, if pending_observation_handoff.is_some() && events.is_empty() && events.sender_strong_count() > 0 => {
                let operation = pending_observation_handoff
                    .take()
                    .expect("completed observation-handoff operation exists");
                if operation.generation != active_generation
                    || runtime_generation.current().await != operation.generation
                    || scene_library_status != SceneLibraryStatus::Ready
                    || lockout.current()
                {
                    continue;
                }
                recall_coordinator.finish_observation_handoff(completion, &event_bus);
            }
            completion = async {
                pending_recall
                    .as_mut()
                    .expect("enabled explicit-recall branch has an operation")
                    .future
                    .as_mut()
                    .await
            }, if pending_recall.is_some() && events.is_empty() && events.sender_strong_count() > 0 => {
                let operation = pending_recall
                    .take()
                    .expect("completed explicit-recall operation exists");
                finish_pending_explicit_recall(
                    operation,
                    completion,
                    &mut pending_recall,
                    &mut recall_coordinator,
                    &peers,
                    active_generation,
                    &runtime_generation,
                    &lockout,
                    &recall_state,
                ).await;
            }
            completion = async {
                pending_abort
                    .as_mut()
                    .expect("enabled abort branch has an operation")
                    .future
                    .as_mut()
                    .await
            }, if pending_abort.is_some() && events.is_empty() && events.sender_strong_count() > 0 => {
                let mut operation = pending_abort
                    .take()
                    .expect("completed abort operation exists");
                let result = if operation.generation == active_generation
                    && runtime_generation.current().await == operation.generation
                    && peers.handles(active_generation).is_some()
                {
                    completion
                } else {
                    Err(AppCommandError::StaleGeneration)
                };
                operation.reply(result);
            }
            completion = async {
                pending_lag_recovery
                    .as_mut()
                    .expect("enabled lag-recovery branch has an operation")
                    .future
                    .as_mut()
                    .await
            }, if pending_lag_recovery.is_some() && events.is_empty() && events.sender_strong_count() > 0 => {
                let operation = pending_lag_recovery
                    .take()
                    .expect("completed lag-recovery operation exists");
                if operation.generation == active_generation
                    && runtime_generation.current().await == operation.generation
                {
                    if completion.settings.is_none()
                        && settings_revision == operation.settings_revision
                    {
                        break;
                    }
                    if let Some(recovered_settings) = completion.settings
                        && settings_revision == operation.settings_revision
                    {
                        settings = recovered_settings;
                    }
                    if scene_list_revision == operation.scene_list_revision
                        && peers.handles(active_generation).is_some()
                        && let Some(snapshot) = completion.snapshot
                        && let Some(scene_list) = authoritative_scene_list(&snapshot)
                    {
                        apply_scene_list(
                            &mut recall_state,
                            &event_bus,
                            active_generation,
                            scene_list,
                        );
                        scene_library_status = SceneLibraryStatus::Ready;
                    }
                }
            }
            completion = async {
                pending_snapshot
                    .as_mut()
                    .expect("enabled pending snapshot branch has an operation")
                    .future
                    .as_mut()
                    .await
            }, if pending_snapshot.is_some() && events.is_empty() && events.sender_strong_count() > 0 => {
                let operation = pending_snapshot
                    .take()
                    .expect("completed pending snapshot operation exists");
                finish_pending_snapshot(
                    operation,
                    completion,
                    active_generation,
                    &runtime_generation,
                    &scene_library_status,
                    &mut recall_state,
                    &event_bus,
                ).await;
            }
            lockout_changed = lockout.changed(), if lockout_open => match lockout_changed {
                Ok(true) => {
                    pending_observation_settings = None;
                    pending_observation_snapshot = None;
                    pending_observation_handoff = None;
                    cancel_pending_explicit_recall(
                        pending_recall.take(),
                        &mut recall_coordinator,
                        "lockout was enabled",
                    );
                    recall_coordinator.cancel("lockout was enabled", true);
                }
                Ok(false) => {}
                Err(_) => {
                    pending_observation_settings = None;
                    pending_observation_snapshot = None;
                    pending_observation_handoff = None;
                    cancel_pending_explicit_recall(
                        pending_recall.take(),
                        &mut recall_coordinator,
                        "lockout state is unavailable",
                    );
                    recall_coordinator.cancel("lockout state is unavailable", true);
                    lockout_open = false;
                }
            },
            _ = recall_timer, if recall_deadline.is_some() => {
                let elapsed = recall_deadline.expect("enabled recall timer has a deadline");
                if elapsed.is_safety() {
                    pending_observation_settings = None;
                    pending_observation_snapshot = None;
                    pending_observation_handoff = None;
                    cancel_pending_explicit_recall(
                        pending_recall.take(),
                        &mut recall_coordinator,
                        "LV1 recall readiness was lost",
                    );
                }
                let should_dispatch = recall_coordinator.handle_deadline(
                    elapsed,
                    tokio::time::Instant::now(),
                );
                if should_dispatch && pending_recall.is_none() {
                    pending_recall = start_next_recall_dispatch(
                        &mut recall_coordinator,
                        &peers,
                        active_generation,
                    );
                    if pending_recall.is_none() && peers.handles(active_generation).is_none() {
                        recall_coordinator.cancel("LV1 state is unavailable", true);
                    }
                }
            }
            _ = pending_scene_settle, if pending_scene_deadline.is_some() && pending_observation_settings.is_none() && pending_observation_snapshot.is_none() && pending_observation_handoff.is_none() && events.is_empty() && events.sender_strong_count() > 0 => {
                if let Some(observation) = recall_coordinator.take_pending_observation() {
                    pending_observation_settings = Some(PendingObservationSettings::new(
                        observation,
                        settings_revision,
                        settings_handle.clone(),
                    ));
                }
            }
        }
        if !scene_ids.iter().copied().eq(recall_state
            .scene_configs()
            .iter()
            .map(|scene| scene.internal_scene_id))
        {
            cues.reconcile(recall_state.scene_configs());
        }
    }

    if let Some(operation) = pending_snapshot {
        operation.cancel("Store scene blocked: Scenes actor stopped");
    }
    if let Some(operation) = pending_abort {
        operation.cancel(AppCommandError::FadeUnavailable);
    }
    cancel_pending_explicit_recall(
        pending_recall,
        &mut recall_coordinator,
        "Scenes actor stopped",
    );
    recall_coordinator.cancel("Scenes actor stopped", true);
    recall_coordinator.clear_late_observations();
}

fn accepts_scene_observation_generation(event_generation: u64, active_generation: u64) -> bool {
    event_generation == active_generation
}

#[allow(clippy::too_many_arguments)]
/**
 * @cc [owner:mixxorz,label:safety] generation-transition-cancels-runtime-intent
 * A generation transition MUST clear runtime library/readiness, pending observations, queued
 * recalls, late-cancellation suppression, and prior-generation peers while preserving the scene
 * document, selection, and clipboard.
 */
fn transition_scene_generation(
    active_generation: &mut u64,
    next_generation: u64,
    cached_scene_list: &mut Option<Vec<crate::lv1::SceneListEntry>>,
    scene_library_status: &mut SceneLibraryStatus,
    recall_state: &mut ScenesState,
    recall_coordinator: &mut RecallCoordinator,
    peers: &ScenesPeers,
    event_bus: &AppEventBus,
) {
    let previous_generation = *active_generation;
    *active_generation = next_generation;
    *cached_scene_list = None;
    *scene_library_status = SceneLibraryStatus::AwaitingPeers;
    recall_state.mark_scene_library_unavailable();
    recall_coordinator.clear_pending_observation();
    recall_coordinator.cancel("LV1 connection generation changed", true);
    recall_coordinator.clear_late_observations();
    if previous_generation != next_generation {
        peers.clear_peers_for_generation(previous_generation);
    }
    publish_scene_state_changed(event_bus, *active_generation, recall_state, false);
}

struct PendingSnapshotOperation {
    generation: u64,
    internal_scene_id: uuid::Uuid,
    reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>>,
    future: Pin<Box<dyn Future<Output = Result<Lv1StateSnapshot, String>> + Send>>,
}

impl PendingSnapshotOperation {
    fn store(
        lv1: Lv1Connection,
        internal_scene_id: uuid::Uuid,
        reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>>,
    ) -> Self {
        let generation = lv1.generation();
        Self {
            generation,
            internal_scene_id,
            reply,
            future: Box::pin(async move {
                lv1.request(|reply| Lv1Command::GetState { reply })
                    .await
                    .map_err(|_| "Store scene blocked: LV1 state is unavailable".to_string())
            }),
        }
    }

    fn cancel(mut self, reason: &str) {
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(Err(reason.to_string()));
        }
    }
}

/**
 * @cc [owner:mixxorz,label:safety;reliability] store-snapshot-pending-operation
 * A store-from-LV1 state request MUST run as the Scenes actor's single pending snapshot operation.
 * While it is pending the actor MUST continue consuming runtime and LV1 facts, lockout changes,
 * and recall deadlines. Disconnect, generation change, session replacement, Abort All, or actor
 * shutdown MUST drop the request and reject its caller before a late snapshot can mutate or publish
 * scene state.
 */
async fn finish_pending_snapshot(
    mut operation: PendingSnapshotOperation,
    result: Result<Lv1StateSnapshot, String>,
    active_generation: u64,
    runtime_generation: &RuntimeGeneration,
    scene_library_status: &SceneLibraryStatus,
    state: &mut ScenesState,
    event_bus: &AppEventBus,
) {
    let result = match result {
        Ok(snapshot)
            if operation.generation == active_generation
                && runtime_generation.current().await == operation.generation
                && *scene_library_status == SceneLibraryStatus::Ready
                && snapshot.connection == ConnectionStatus::Connected =>
        {
            let changed = state
                .store_scene_config(operation.internal_scene_id, &snapshot.channels)
                .map_err(|error| error.to_string());
            changed.map(|changed| {
                if changed {
                    publish_scene_state_changed(event_bus, operation.generation, state, true);
                }
                ScenesCommandResult { changed }
            })
        }
        Ok(_) => Err("Store scene blocked: scene library is unavailable".to_string()),
        Err(error) => Err(error),
    };
    if let Some(reply) = operation.reply.take() {
        let _ = reply.send(result);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneLibraryStatus {
    AwaitingPeers,
    AwaitingSceneList,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScenesCommandDispatch {
    Continue,
    Shutdown,
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_scenes_command(
    command: ScenesCommand,
    recall_state: &mut ScenesState,
    _recall_coordinator: &mut RecallCoordinator,
    peers: &ScenesPeers,
    event_bus: &AppEventBus,
    generation: u64,
    runtime_generation: &RuntimeGeneration,
    _lockout: &ShowLockoutReader,
    cached_scene_list: &mut Option<Vec<crate::lv1::SceneListEntry>>,
    scene_library_status: &mut SceneLibraryStatus,
) -> ScenesCommandDispatch {
    match command {
        ScenesCommand::GetSessionDocument { .. } | ScenesCommand::ReplaceSessionDocument { .. } => {
            unreachable!("handled by the document owner")
        }
        ScenesCommand::GetSceneConfig {
            internal_scene_id,
            reply,
        } => {
            let _ = reply.send(recall_state.get_scene_config(internal_scene_id));
        }
        ScenesCommand::InitialProjectionState { reply } => {
            let _ = reply.send(recall_state.projection_state());
        }
        ScenesCommand::RuntimePeersReady {
            generation: ready_generation,
            initial_scene_list,
            reply,
        } => {
            let result = runtime_generation
                .if_current(ready_generation, || {
                    if ready_generation != generation || peers.handles(generation).is_none() {
                        return Err(AppCommandError::ScenesUnavailable);
                    }
                    let scene_list = match cached_scene_list.take() {
                        Some(cached) => Some(cached),
                        None if initial_scene_list.is_empty() => None,
                        None => Some(initial_scene_list),
                    };
                    if let Some(scene_list) = scene_list {
                        apply_scene_list(recall_state, event_bus, generation, scene_list);
                        *scene_library_status = SceneLibraryStatus::Ready;
                    } else {
                        *scene_library_status = SceneLibraryStatus::AwaitingSceneList;
                    }
                    Ok(())
                })
                .await
                .unwrap_or(Err(AppCommandError::ScenesUnavailable));
            let _ = reply.send(result);
        }
        ScenesCommand::SetSceneDuration {
            internal_scene_id,
            duration_ms,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                true,
                |state| state.set_scene_duration_ms(internal_scene_id, duration_ms),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::SetSceneScopeFadersEnabled {
            internal_scene_id,
            enabled,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                true,
                |state| state.set_scene_scope_faders_enabled(internal_scene_id, enabled),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::SetSceneScopePanEnabled {
            internal_scene_id,
            enabled,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                true,
                |state| state.set_scene_scope_pan_enabled(internal_scene_id, enabled),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::LinkSceneConfig {
            source_internal_scene_id,
            target_scene_index,
            overwrite_existing,
            reply,
        } => {
            let result = runtime_generation
                .if_current(generation, || {
                    if *scene_library_status != SceneLibraryStatus::Ready {
                        return Err(AppCommandError::ScenesUnavailable.to_string());
                    }
                    mutate_scene_state(
                        recall_state,
                        true,
                        |state| {
                            state.link_scene_config_by_index(
                                source_internal_scene_id,
                                target_scene_index,
                                overwrite_existing,
                            )
                        },
                        event_bus,
                        generation,
                    )
                })
                .await
                .unwrap_or_else(|| Err(AppCommandError::ScenesUnavailable.to_string()));
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::DeleteSceneConfig {
            internal_scene_id,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                true,
                |state| state.delete_scene_config(internal_scene_id),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::SetChannelScoped {
            internal_scene_id,
            group,
            channel,
            scoped,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                true,
                |state| state.set_channel_scoped(internal_scene_id, group, channel, scoped),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::SetAllChannelsScoped {
            internal_scene_id,
            scoped,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                true,
                |state| state.set_all_channels_scoped(internal_scene_id, scoped),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::SelectSceneConfig {
            internal_scene_id,
            reply,
        } => {
            let result = recall_state
                .select_scene_config(internal_scene_id)
                .map(|changed| {
                    if changed {
                        publish_scene_state_changed(event_bus, generation, recall_state, false);
                    }
                    SelectedSceneResult {
                        scene: recall_state.get_scene_config(internal_scene_id).unwrap(),
                    }
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::CopySceneSettings {
            source_internal_scene_id,
            reply,
        } => {
            let result = copy_scene_settings(
                recall_state,
                source_internal_scene_id,
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::PasteSceneSettings {
            destination_internal_scene_id,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                true,
                |state| state.paste_scene_settings(destination_internal_scene_id),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::StoreSceneConfigFromCurrentLv1 { .. } => {
            unreachable!("store snapshot operations are started by the actor loop")
        }
        ScenesCommand::RecallScene { .. } => {
            unreachable!("explicit recall operations are started by the actor loop")
        }
        ScenesCommand::AbortAll { .. } => {
            unreachable!("abort operations are started by the actor loop")
        }
        ScenesCommand::Shutdown => return ScenesCommandDispatch::Shutdown,
    }
    ScenesCommandDispatch::Continue
}

enum PendingExplicitRecallContext {
    Admission(PendingRecallAdmission),
    DispatchSnapshot(PendingRecallDispatch),
    DispatchCommand(PreparedRecallDispatch),
}

enum PendingExplicitRecallCompletion {
    Snapshot(Result<Lv1StateSnapshot, AppCommandError>),
    Dispatch(Result<RecallSceneDispatch, AppCommandError>),
}

struct PendingExplicitRecallOperation {
    generation: u64,
    context: PendingExplicitRecallContext,
    future: Pin<Box<dyn Future<Output = PendingExplicitRecallCompletion> + Send>>,
}

impl PendingExplicitRecallOperation {
    fn snapshot_for_admission(lv1: Lv1Connection, admission: PendingRecallAdmission) -> Self {
        Self::snapshot(lv1, PendingExplicitRecallContext::Admission(admission))
    }

    fn snapshot_for_dispatch(lv1: Lv1Connection, dispatch: PendingRecallDispatch) -> Self {
        Self::snapshot(
            lv1,
            PendingExplicitRecallContext::DispatchSnapshot(dispatch),
        )
    }

    fn snapshot(lv1: Lv1Connection, context: PendingExplicitRecallContext) -> Self {
        let generation = lv1.generation();
        Self {
            generation,
            context,
            future: Box::pin(async move {
                let result = lv1
                    .request(|reply| Lv1Command::GetState { reply })
                    .await
                    .map_err(|error| match error {
                        AppCommandError::Lv1Unavailable => AppCommandError::CommandFailed(
                            "Recall blocked: LV1 state is unavailable".to_string(),
                        ),
                        other => other,
                    });
                PendingExplicitRecallCompletion::Snapshot(result)
            }),
        }
    }

    fn dispatch(
        lv1: Lv1Connection,
        lockout: ShowLockoutReader,
        prepared: PreparedRecallDispatch,
    ) -> Self {
        let generation = lv1.generation();
        let scene_index = prepared.scene_index();
        Self {
            generation,
            context: PendingExplicitRecallContext::DispatchCommand(prepared),
            future: Box::pin(async move {
                let result = lv1
                    .request_checked(
                        |reply| Lv1Command::RecallScene {
                            scene_index,
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
                PendingExplicitRecallCompletion::Dispatch(result)
            }),
        }
    }
}

fn start_next_recall_dispatch(
    coordinator: &mut RecallCoordinator,
    peers: &ScenesPeers,
    generation: u64,
) -> Option<PendingExplicitRecallOperation> {
    let dispatch = coordinator.begin_dispatch()?;
    let Some(handles) = peers.handles(generation) else {
        coordinator.cancel_pending_dispatch(dispatch, "LV1 state is unavailable");
        return None;
    };
    Some(PendingExplicitRecallOperation::snapshot_for_dispatch(
        handles.lv1,
        dispatch,
    ))
}

fn cancel_pending_explicit_recall(
    operation: Option<PendingExplicitRecallOperation>,
    coordinator: &mut RecallCoordinator,
    reason: &str,
) {
    let Some(operation) = operation else {
        return;
    };
    match operation.context {
        PendingExplicitRecallContext::Admission(admission) => {
            coordinator.cancel_admission(admission, reason);
        }
        PendingExplicitRecallContext::DispatchSnapshot(dispatch) => {
            coordinator.cancel_pending_dispatch(dispatch, reason);
        }
        PendingExplicitRecallContext::DispatchCommand(prepared) => {
            coordinator.cancel_prepared_dispatch(prepared, reason, true);
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn finish_pending_explicit_recall(
    operation: PendingExplicitRecallOperation,
    completion: PendingExplicitRecallCompletion,
    pending: &mut Option<PendingExplicitRecallOperation>,
    coordinator: &mut RecallCoordinator,
    peers: &ScenesPeers,
    active_generation: u64,
    runtime_generation: &RuntimeGeneration,
    lockout: &ShowLockoutReader,
    recall_state: &ScenesState,
) {
    if operation.generation != active_generation
        || runtime_generation.current().await != operation.generation
        || peers.handles(active_generation).is_none()
    {
        cancel_pending_explicit_recall(
            Some(operation),
            coordinator,
            "LV1 connection generation changed",
        );
        coordinator.clear_late_observations();
        return;
    }

    match (operation.context, completion) {
        (
            PendingExplicitRecallContext::Admission(admission),
            PendingExplicitRecallCompletion::Snapshot(Ok(snapshot)),
        ) => {
            if coordinator.finish_admission(admission, lockout, &recall_state.snapshot(), &snapshot)
            {
                *pending = start_next_recall_dispatch(coordinator, peers, active_generation);
            }
        }
        (
            PendingExplicitRecallContext::Admission(admission),
            PendingExplicitRecallCompletion::Snapshot(Err(error)),
        ) => {
            if coordinator.fail_admission(admission, error) {
                *pending = start_next_recall_dispatch(coordinator, peers, active_generation);
            }
        }
        (
            PendingExplicitRecallContext::DispatchSnapshot(dispatch),
            PendingExplicitRecallCompletion::Snapshot(Ok(snapshot)),
        ) => {
            match coordinator.prepare_dispatch(
                dispatch,
                active_generation,
                lockout,
                &recall_state.snapshot(),
                &snapshot,
            ) {
                Ok(prepared) => {
                    let handles = peers
                        .handles(active_generation)
                        .expect("current recall peers were checked");
                    *pending = Some(PendingExplicitRecallOperation::dispatch(
                        handles.lv1,
                        lockout.clone(),
                        prepared,
                    ));
                }
                Err(()) => {
                    *pending = start_next_recall_dispatch(coordinator, peers, active_generation);
                }
            }
        }
        (
            PendingExplicitRecallContext::DispatchSnapshot(dispatch),
            PendingExplicitRecallCompletion::Snapshot(Err(error)),
        ) => {
            let (reason, clear_late) = if error == AppCommandError::StaleGeneration {
                ("LV1 connection generation changed", true)
            } else {
                ("LV1 state is unavailable", false)
            };
            coordinator.cancel_pending_dispatch(dispatch, reason);
            if clear_late {
                coordinator.clear_late_observations();
            }
        }
        (
            PendingExplicitRecallContext::DispatchCommand(prepared),
            PendingExplicitRecallCompletion::Dispatch(result),
        ) => {
            coordinator.finish_dispatch(prepared, result);
        }
        (context, _) => {
            cancel_pending_explicit_recall(
                Some(PendingExplicitRecallOperation {
                    generation: operation.generation,
                    context,
                    future: Box::pin(std::future::pending()),
                }),
                coordinator,
                "LV1 recall operation became inconsistent",
            );
        }
    }
}

/**
 * @cc [owner:mixxorz,label:safety;reliability] abort-all-pending-operation
 * Abort All MUST synchronously cancel coordinated recall intent before its Fade request begins.
 * Fade mailbox admission and acknowledgement MUST run as an actor-owned pending operation while
 * runtime, LV1, Settings, lockout, and recall-deadline inputs remain serviced. Scene and cue
 * commands MUST remain FIFO-gated until completion. Generation MUST be rechecked after Fade
 * mailbox reservation and immediately before sending. Generation change, disconnect, or shutdown
 * MUST drop stale work, and a late acknowledgement MUST NOT be reported as current success.
 */
struct PendingAbortOperation {
    generation: u64,
    reply: Option<oneshot::Sender<Result<(), AppCommandError>>>,
    future: Pin<Box<dyn Future<Output = Result<(), AppCommandError>> + Send>>,
}

impl PendingAbortOperation {
    fn new(
        generation: u64,
        peers: Option<(Lv1Connection, FadeEngineHandle)>,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    ) -> Self {
        Self {
            generation,
            reply: Some(reply),
            future: Box::pin(async move {
                let Some((lv1, fade)) = peers else {
                    return Err(AppCommandError::FadeUnavailable);
                };
                let permit = fade
                    .reserve()
                    .await
                    .map_err(|_| AppCommandError::FadeUnavailable)?;
                let (fade_reply, fade_result) = oneshot::channel();
                lv1.if_current(|| {
                    permit.send(FadeCommand::AbortAll {
                        reply: Some(fade_reply),
                    });
                })
                .await
                .ok_or(AppCommandError::StaleGeneration)?;
                let result = fade_result
                    .await
                    .map_err(|_| AppCommandError::ReplyChannelClosed)?;
                lv1.ensure_current().await?;
                result
            }),
        }
    }

    fn reply(&mut self, result: Result<(), AppCommandError>) {
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(result);
        }
    }

    fn cancel(mut self, error: AppCommandError) {
        self.reply(Err(error));
    }
}

/**
 * @cc [owner:mixxorz,label:safety;reliability] settled-observation-settings-pending
 * A settled scene observation's Settings refresh MUST be an actor-owned pending operation polled
 * by the main select. Runtime, LV1, Settings, lockout, and recall-deadline inputs MUST remain
 * responsive while it is pending. Cancellation MUST drop the observation token, and completion
 * MUST be generation-fenced and MUST NOT overwrite a newer Settings fact.
 */
struct PendingObservationSettings {
    generation: u64,
    settings_revision: u64,
    observation: PendingSceneObservation,
    future: Pin<Box<dyn Future<Output = Option<AppSettings>> + Send>>,
}

impl PendingObservationSettings {
    fn new(
        observation: PendingSceneObservation,
        settings_revision: u64,
        settings_handle: SettingsHandle,
    ) -> Self {
        Self {
            generation: observation.generation(),
            settings_revision,
            observation,
            future: Box::pin(async move { request_recovery_settings(&settings_handle).await }),
        }
    }
}

/**
 * @cc [owner:mixxorz,label:safety;reliability] settled-observation-snapshot-pending
 * After synchronous observation prechecks, exact fresh LV1 snapshot acquisition and retries MUST
 * run as an actor-owned pending operation. Facts, lockout, and recall deadlines MUST remain
 * responsive, while cancellation MUST drop the coordinator token before stale completion can
 * publish, log, or hand off Fade work.
 */
struct PendingObservationSnapshot {
    generation: u64,
    prepared: PreparedSceneObservation,
    future: Pin<Box<dyn Future<Output = Result<Lv1StateSnapshot, AppCommandError>> + Send>>,
}

impl PendingObservationSnapshot {
    fn new(lv1: Lv1Connection, prepared: PreparedSceneObservation) -> Self {
        let generation = prepared.generation();
        let scene = prepared.scene();
        let safety_deadline = prepared.safety_deadline();
        Self {
            generation,
            prepared,
            future: Box::pin(
                async move { fresh_lv1_snapshot(&lv1, &scene, safety_deadline).await },
            ),
        }
    }
}

/**
 * @cc [owner:mixxorz,label:safety;reliability] settled-observation-fade-pending
 * Settled-observation Fade mailbox admission and acknowledgement MUST run as an actor-owned
 * pending operation. The permit path MUST recheck generation, lockout, and readiness deadline
 * immediately before publication and send. Cancellation MUST drop queued readiness ownership and
 * prevent late capacity or acknowledgement from committing or reporting current success.
 */
struct PendingObservationHandoff {
    generation: u64,
    future: Pin<Box<dyn Future<Output = ObservationHandoffCompletion> + Send>>,
}

impl PendingObservationHandoff {
    #[allow(clippy::too_many_arguments)]
    fn new(
        lv1: Lv1Connection,
        fade: FadeEngineHandle,
        event_bus: AppEventBus,
        lockout: ShowLockoutReader,
        #[cfg(test)] before_fade_handoff: Option<BeforeFadeHandoff>,
        prepared: PreparedObservationHandoff,
    ) -> Self {
        let generation = prepared.generation;
        Self {
            generation,
            future: Box::pin(async move {
                #[cfg(test)]
                if let Some(BeforeFadeHandoff { reached, resume }) = before_fade_handoff {
                    let _ = reached.send(());
                    let _ = resume.await;
                }

                let PreparedObservationHandoff {
                    generation,
                    scene_label,
                    queued_readiness,
                    request,
                } = prepared;
                let deadline = match &request {
                    ObservationFadeRequest::Start { readiness, .. }
                    | ObservationFadeRequest::Wait { readiness, .. } => readiness.deadline,
                };
                let publish_bus = event_bus.clone();
                let publish_label = scene_label.clone();
                let result = match request {
                    ObservationFadeRequest::Start {
                        fade_config,
                        same_scene_behavior,
                        readiness,
                    } => {
                        let target_count = fade_config.targets.len();
                        send_fade_checked(
                            &fade,
                            &lv1,
                            &lockout,
                            deadline,
                            move || {
                                tracing::debug!(event = "scene_recall_ready", scene = %publish_label, target_count, "Scene recall ready for {publish_label}");
                                tracing::debug!(event = "scene_recall_start_requested", scene = %publish_label, "Scene recall start requested for {publish_label}");
                                publish_bus.publish_scenes(
                                    generation,
                                    ScenesEvent::Ready {
                                        scene_label: publish_label.clone(),
                                        target_count,
                                    },
                                );
                                publish_bus.publish_scenes(
                                    generation,
                                    ScenesEvent::StartRequested {
                                        scene_label: publish_label,
                                    },
                                );
                            },
                            |reply| FadeCommand::RecallSceneFade {
                                config: fade_config,
                                same_scene_behavior,
                                readiness,
                                reply: Some(reply),
                            },
                        )
                        .await
                    }
                    ObservationFadeRequest::Wait { scene, readiness } => {
                        send_fade_checked(
                            &fade,
                            &lv1,
                            &lockout,
                            deadline,
                            || {},
                            |reply| FadeCommand::WaitForRecallReadiness {
                                scene,
                                readiness,
                                reply: Some(reply),
                            },
                        )
                        .await
                    }
                };
                ObservationHandoffCompletion {
                    result,
                    queued_readiness,
                    generation,
                    scene_label,
                }
            }),
        }
    }
}

struct PendingLagRecovery {
    generation: u64,
    settings_revision: u64,
    scene_list_revision: u64,
    future: Pin<Box<dyn Future<Output = LagRecoveryCompletion> + Send>>,
}

struct LagRecoveryCompletion {
    settings: Option<AppSettings>,
    snapshot: Option<Lv1StateSnapshot>,
}

impl PendingLagRecovery {
    fn new(
        generation: u64,
        settings_revision: u64,
        scene_list_revision: u64,
        settings_handle: SettingsHandle,
        lv1: Option<Lv1Connection>,
    ) -> Self {
        Self {
            generation,
            settings_revision,
            scene_list_revision,
            future: Box::pin(async move {
                let settings = request_recovery_settings(&settings_handle).await;
                let snapshot = match (&settings, lv1) {
                    (Some(_), Some(lv1)) => lv1
                        .request(|reply| Lv1Command::GetState { reply })
                        .await
                        .ok(),
                    _ => None,
                };
                LagRecoveryCompletion { settings, snapshot }
            }),
        }
    }
}

async fn request_recovery_settings(settings_handle: &SettingsHandle) -> Option<AppSettings> {
    let (reply, rx) = oneshot::channel();
    let result = match settings_handle
        .send(SettingsCommand::GetSettings { reply })
        .await
    {
        Ok(()) => rx.await.ok(),
        Err(_) => None,
    };
    if result.is_none() {
        tracing::error!(
            event = "scene_recall_settings_unavailable",
            "Scene recall automation stopped because current settings are unavailable"
        );
    }
    result
}

fn authoritative_scene_list(
    snapshot: &Lv1StateSnapshot,
) -> Option<Vec<crate::lv1::SceneListEntry>> {
    (snapshot.connection == ConnectionStatus::Connected).then(|| snapshot.scene_list.clone())
}

fn drain_retained_events(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
    loop {
        match events.try_recv() {
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
            | Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
        }
    }
}

fn apply_scene_list(
    recall_state: &mut ScenesState,
    event_bus: &AppEventBus,
    generation: u64,
    scene_list: Vec<crate::lv1::SceneListEntry>,
) {
    let before = recall_state.scene_configs().to_vec();
    let changed = recall_state.observe_and_align_scene_list(
        true,
        generation,
        scene_list.clone(),
        tokio::time::Instant::now(),
    );
    if changed {
        log_scene_alignment(&before, recall_state, &scene_list);
    }
    publish_scene_state_changed(event_bus, generation, recall_state, changed);
}

/**
 * @cc [owner:mixxorz,label:persistence] persisted-edit-classification
 * `persisted_scene_edit` MUST be true only for changes that independently dirty the show. Selection,
 * clipboard availability, readiness, and other projection/runtime-only changes MUST publish it as
 * false, even though selection is included when a session is otherwise serialized.
 */
fn publish_scene_state_changed(
    event_bus: &AppEventBus,
    generation: u64,
    state: &ScenesState,
    persisted_scene_edit: bool,
) {
    event_bus.publish_scenes(
        generation,
        ScenesEvent::StateChanged {
            state: state.projection_state(),
            persisted_scene_edit,
        },
    );
}

fn log_scene_alignment(
    before: &[crate::scenes::SceneConfig],
    state: &ScenesState,
    scene_list: &[crate::lv1::SceneListEntry],
) {
    tracing::debug!(
        event = "session_scene_alignment",
        "{}",
        scene_alignment_diagnostic(before, state.scene_configs(), scene_list)
    );
}

fn mutate_scene_state<F>(
    state: &mut ScenesState,
    persisted_scene_edit: bool,
    op: F,
    event_bus: &AppEventBus,
    generation: u64,
) -> Result<ScenesCommandResult, String>
where
    F: FnOnce(&mut ScenesState) -> Result<bool, String>,
{
    let changed = op(state)?;
    if changed {
        publish_scene_state_changed(event_bus, generation, state, persisted_scene_edit);
    }
    Ok(ScenesCommandResult { changed })
}

fn copy_scene_settings(
    state: &mut ScenesState,
    source_internal_scene_id: uuid::Uuid,
    event_bus: &AppEventBus,
    generation: u64,
) -> Result<ScenesCommandResult, String> {
    let result = state.copy_scene_settings(source_internal_scene_id)?;
    if result.availability_changed {
        publish_scene_state_changed(event_bus, generation, state, false);
    }
    Ok(ScenesCommandResult {
        changed: result.contents_changed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::{
        FadeCommand, FadeConfig, FadeCurve, FadeEngineHandle, FadeParameter, FadeSceneIdentity,
        FadeTarget, RecallReadinessError, SameSceneRecallBehavior,
    };
    use crate::lv1::{
        Lv1ActorError, Lv1ActorHandle, Lv1Event, Lv1StateSnapshot, RecallSceneDispatch,
        SceneListEntry, SceneObservation, SceneState,
    };
    use crate::scenes::events::ScenesEvent;
    use crate::scenes::{
        ChannelConfig, ChannelRef, RecallSceneResult, SceneConfig, SceneDocument, SceneScopeToggles,
    };
    use crate::settings::{AppSettings, SettingsCommand, SettingsHandle};
    use crate::test_support::TracingCapture;
    use std::collections::VecDeque;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;
    use tracing::Level;

    fn test_lockout_reader() -> ShowLockoutReader {
        let (show, task, _peers, lockout) = crate::show::build_show_actor(AppEventBus::default());
        task.spawn();
        // Keep the command sender alive for the test binary so the lockout watch models a running
        // Show actor rather than nondeterministically racing a closed watch against Fade admission.
        std::mem::forget(show);
        lockout
    }

    struct ObservedLv1Recall {
        scene_index: i32,
        reply: oneshot::Sender<Result<RecallSceneDispatch, Lv1ActorError>>,
    }

    impl ObservedLv1Recall {
        fn reply(self, result: Result<RecallSceneDispatch, Lv1ActorError>) {
            let _ = self.reply.send(result);
        }
    }

    struct RecallCoordinatorFixture {
        handle: ScenesHandle,
        show: crate::show::ShowStateHandle,
        event_bus: AppEventBus,
        runtime_generation: RuntimeGeneration,
        snapshot: tokio::sync::watch::Sender<Lv1StateSnapshot>,
        close_lv1_peer: Option<oneshot::Sender<()>>,
        lv1_peer_closed: Option<oneshot::Receiver<()>>,
        lv1_recalls: tokio::sync::mpsc::Receiver<ObservedLv1Recall>,
        fade_commands: tokio::sync::mpsc::Receiver<QueueFadeCommand>,
    }

    struct RuntimeSignalFixture {
        handle: ScenesHandle,
        event_bus: AppEventBus,
        snapshot: tokio::sync::watch::Sender<Lv1StateSnapshot>,
        lv1_recalls: tokio::sync::mpsc::Receiver<ObservedLv1Recall>,
        fade_commands: tokio::sync::mpsc::Receiver<QueueFadeCommand>,
    }

    struct FadeReservationLockoutFixture {
        handle: ScenesHandle,
        show: crate::show::ShowStateHandle,
        lockout: ShowLockoutReader,
        event_bus: AppEventBus,
        snapshot: tokio::sync::watch::Sender<Lv1StateSnapshot>,
        lv1_recalls: tokio::sync::mpsc::Receiver<ObservedLv1Recall>,
        state_requests: tokio::sync::mpsc::Receiver<()>,
        fade: FadeEngineHandle,
        fade_commands: tokio::sync::mpsc::Receiver<FadeCommand>,
    }

    impl FadeReservationLockoutFixture {
        async fn connected_with_scenes(scene_configs: Vec<SceneConfig>) -> Self {
            let event_bus = AppEventBus::default();
            let (show, show_task, _show_peers, lockout) =
                crate::show::build_show_actor(event_bus.clone());
            show_task.spawn();

            let initial_snapshot = Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: scene_configs
                    .iter()
                    .map(|scene| SceneListEntry {
                        index: scene.scene_index.unwrap(),
                        name: scene.scene_name.clone(),
                    })
                    .collect(),
                channels: vec![],
                ping_sequence: 10,
            };
            let (snapshot, snapshot_rx) = tokio::sync::watch::channel(initial_snapshot);
            let (state_requested_tx, state_requests) = tokio::sync::mpsc::channel(8);
            let (recall_tx, lv1_recalls) = tokio::sync::mpsc::channel(8);
            let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
            tokio::spawn(async move {
                while let Some(command) = lv1_rx.recv().await {
                    match command {
                        Lv1Command::GetState { reply } => {
                            let _ = state_requested_tx.send(()).await;
                            let _ = reply.send(snapshot_rx.borrow().clone());
                        }
                        Lv1Command::RecallScene {
                            scene_index,
                            reply: Some(reply),
                        } => {
                            recall_tx
                                .send(ObservedLv1Recall { scene_index, reply })
                                .await
                                .unwrap();
                        }
                        _ => panic!("unexpected LV1 command"),
                    }
                }
            });

            let runtime_generation = RuntimeGeneration::new();
            runtime_generation.set(1).await;
            let lv1 = crate::lv1::test_actor_handle(lv1_tx);
            let (fade_tx, fade_commands) = tokio::sync::mpsc::channel(1);
            let fade = fade_tx;
            let (handle, task, peers) = build_scenes_actor(
                1,
                runtime_generation,
                event_bus.clone(),
                event_bus.subscribe(),
                fake_settings_handle(AppSettings::default()),
                AppSettings::default(),
                lockout.clone(),
            );
            peers.set_peers_for_generation(1, lv1, fade.clone());
            task.spawn();
            install_scene_document(
                &handle,
                SceneDocument {
                    scene_configs,
                    selected_scene_internal_id: None,
                },
            )
            .await;

            Self {
                handle,
                show,
                lockout,
                event_bus,
                snapshot,
                lv1_recalls,
                state_requests,
                fade,
                fade_commands,
            }
        }

        async fn send_recall(
            &self,
            internal_scene_id: uuid::Uuid,
        ) -> oneshot::Receiver<Result<RecallSceneResult, AppCommandError>> {
            let (reply, rx) = oneshot::channel();
            self.handle
                .send(ScenesCommand::RecallScene {
                    internal_scene_id,
                    reply,
                })
                .await
                .unwrap();
            rx
        }

        async fn next_lv1_recall(&mut self) -> ObservedLv1Recall {
            self.lv1_recalls.recv().await.expect("expected LV1 recall")
        }

        fn set_current_scene(&self, scene: SceneState) {
            let mut snapshot = self.snapshot.borrow().clone();
            snapshot.scene = Some(scene);
            self.snapshot.send_replace(snapshot);
        }

        fn set_snapshot(&self, snapshot: Lv1StateSnapshot) {
            self.snapshot.send_replace(snapshot);
        }

        fn close_fade_peer(&mut self) {
            let (_sender, replacement) = tokio::sync::mpsc::channel(1);
            drop(std::mem::replace(&mut self.fade_commands, replacement));
        }

        fn publish_scene_observation(&self, sequence: u64, scene: SceneState) {
            self.event_bus.publish(AppEvent::Lv1 {
                generation: 1,
                event: Lv1Event::SceneChanged(SceneObservation { sequence, scene }),
            });
        }
    }

    impl RecallCoordinatorFixture {
        async fn connected_with_scenes(scene_configs: Vec<SceneConfig>) -> Self {
            Self::connected_with_scenes_and_settings(scene_configs, AppSettings::default()).await
        }

        async fn connected_with_scenes_and_settings(
            scene_configs: Vec<SceneConfig>,
            settings: AppSettings,
        ) -> Self {
            Self::connected_with_scenes_settings_and_handoff_gate(scene_configs, settings, None)
                .await
        }

        async fn connected_with_scenes_and_handoff_gate(
            scene_configs: Vec<SceneConfig>,
            before_fade_handoff: Option<BeforeFadeHandoff>,
        ) -> Self {
            Self::connected_with_scenes_settings_and_handoff_gate(
                scene_configs,
                AppSettings::default(),
                before_fade_handoff,
            )
            .await
        }

        async fn connected_with_scenes_settings_and_handoff_gate(
            scene_configs: Vec<SceneConfig>,
            settings: AppSettings,
            before_fade_handoff: Option<BeforeFadeHandoff>,
        ) -> Self {
            let event_bus = AppEventBus::default();
            let (show, show_task, _show_peers, lockout) =
                crate::show::build_show_actor(event_bus.clone());
            show_task.spawn();

            let initial_snapshot = Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: scene_configs
                    .iter()
                    .map(|scene| SceneListEntry {
                        index: scene.scene_index.unwrap(),
                        name: scene.scene_name.clone(),
                    })
                    .collect(),
                channels: vec![],
                ping_sequence: 10,
            };
            let (snapshot, snapshot_rx) = tokio::sync::watch::channel(initial_snapshot);
            let (recall_tx, lv1_recalls) = tokio::sync::mpsc::channel(8);
            let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
            let (close_lv1_peer, close_lv1_peer_rx) = oneshot::channel();
            let (lv1_peer_closed, lv1_peer_closed_rx) = oneshot::channel();
            tokio::spawn(async move {
                tokio::select! {
                    biased;
                    _ = close_lv1_peer_rx => {
                        let _ = lv1_peer_closed.send(());
                    }
                    _ = async {
                        while let Some(command) = lv1_rx.recv().await {
                            match command {
                                Lv1Command::GetState { reply } => {
                                    let _ = reply.send(snapshot_rx.borrow().clone());
                                }
                                Lv1Command::RecallScene {
                                    scene_index,
                                    reply: Some(reply),
                                } => {
                                    recall_tx
                                        .send(ObservedLv1Recall { scene_index, reply })
                                        .await
                                        .unwrap();
                                }
                                Lv1Command::RecallScene { reply: None, .. } => {
                                    panic!("queued recalls require an LV1 reply");
                                }
                                _ => panic!("unexpected LV1 command"),
                            }
                        }
                    } => {}
                }
            });

            let runtime_generation = RuntimeGeneration::new();
            runtime_generation.set(1).await;
            let lv1 = crate::lv1::test_actor_handle(lv1_tx);
            let (fade, fade_commands) = fake_queue_fade_handle(event_bus.clone());
            let (handle, task, peers) = match before_fade_handoff {
                Some(before_fade_handoff) => build_scenes_actor_with_before_fade_handoff(
                    1,
                    runtime_generation.clone(),
                    event_bus.clone(),
                    event_bus.subscribe(),
                    fake_settings_handle(settings.clone()),
                    settings.clone(),
                    lockout,
                    before_fade_handoff,
                ),
                None => build_scenes_actor(
                    1,
                    runtime_generation.clone(),
                    event_bus.clone(),
                    event_bus.subscribe(),
                    fake_settings_handle(settings.clone()),
                    settings,
                    lockout,
                ),
            };
            peers.set_peers_for_generation(1, lv1, fade);
            task.spawn();
            install_scene_document(
                &handle,
                SceneDocument {
                    scene_configs,
                    selected_scene_internal_id: None,
                },
            )
            .await;

            Self {
                handle,
                show,
                event_bus,
                runtime_generation,
                snapshot,
                close_lv1_peer: Some(close_lv1_peer),
                lv1_peer_closed: Some(lv1_peer_closed_rx),
                lv1_recalls,
                fade_commands,
            }
        }

        async fn send_recall(
            &self,
            internal_scene_id: uuid::Uuid,
        ) -> oneshot::Receiver<Result<RecallSceneResult, AppCommandError>> {
            let (reply, rx) = oneshot::channel();
            self.handle
                .send(ScenesCommand::RecallScene {
                    internal_scene_id,
                    reply,
                })
                .await
                .unwrap();
            rx
        }

        async fn next_lv1_recall(&mut self) -> ObservedLv1Recall {
            self.lv1_recalls.recv().await.expect("expected LV1 recall")
        }

        fn try_next_lv1_recall(&mut self) -> Option<ObservedLv1Recall> {
            self.lv1_recalls.try_recv().ok()
        }

        async fn next_fade_command(&mut self) -> QueueFadeCommand {
            self.fade_commands
                .recv()
                .await
                .expect("expected Fade command")
        }

        fn set_snapshot(&self, snapshot: Lv1StateSnapshot) {
            self.snapshot.send_replace(snapshot);
        }

        fn set_current_scene(&self, scene: SceneState) {
            let mut snapshot = self.snapshot.borrow().clone();
            snapshot.scene = Some(scene);
            self.snapshot.send_replace(snapshot);
        }

        fn publish_scene_observation(&self, generation: u64, sequence: u64, scene: SceneState) {
            self.event_bus.publish(AppEvent::Lv1 {
                generation,
                event: Lv1Event::SceneChanged(SceneObservation { sequence, scene }),
            });
        }

        fn publish_ping(&self, generation: u64, sequence: u64) {
            self.event_bus.publish(AppEvent::Lv1 {
                generation,
                event: Lv1Event::PingReceived { sequence },
            });
        }

        async fn close_lv1_peer(&mut self) {
            self.close_lv1_peer
                .take()
                .expect("LV1 peer should remain available")
                .send(())
                .expect("LV1 peer should accept closure");
            self.lv1_peer_closed
                .take()
                .expect("LV1 peer closure should be observable")
                .await
                .expect("LV1 peer should close its command receiver");
        }
    }

    impl RuntimeSignalFixture {
        #[allow(clippy::too_many_arguments)]
        async fn connected_with_scenes(
            scene_configs: Vec<SceneConfig>,
            event_bus: AppEventBus,
            events: tokio::sync::broadcast::Receiver<AppEvent>,
            lockout: ShowLockoutReader,
            before_fade_handoff: Option<BeforeFadeHandoff>,
        ) -> Self {
            let initial_snapshot = Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: scene_configs
                    .iter()
                    .map(|scene| SceneListEntry {
                        index: scene.scene_index.unwrap(),
                        name: scene.scene_name.clone(),
                    })
                    .collect(),
                channels: vec![],
                ping_sequence: 10,
            };
            let (snapshot, snapshot_rx) = tokio::sync::watch::channel(initial_snapshot);
            let (recall_tx, lv1_recalls) = tokio::sync::mpsc::channel(8);
            let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
            tokio::spawn(async move {
                while let Some(command) = lv1_rx.recv().await {
                    match command {
                        Lv1Command::GetState { reply } => {
                            let _ = reply.send(snapshot_rx.borrow().clone());
                        }
                        Lv1Command::RecallScene {
                            scene_index,
                            reply: Some(reply),
                        } => {
                            recall_tx
                                .send(ObservedLv1Recall { scene_index, reply })
                                .await
                                .unwrap();
                        }
                        _ => panic!("unexpected LV1 command"),
                    }
                }
            });

            let runtime_generation = RuntimeGeneration::new();
            runtime_generation.set(1).await;
            let lv1 = crate::lv1::test_actor_handle(lv1_tx);
            let (fade, fade_commands) = fake_queue_fade_handle(event_bus.clone());
            let (handle, task, peers) = match before_fade_handoff {
                Some(before_fade_handoff) => build_scenes_actor_with_before_fade_handoff(
                    1,
                    runtime_generation,
                    event_bus.clone(),
                    events,
                    fake_settings_handle(AppSettings::default()),
                    AppSettings::default(),
                    lockout,
                    before_fade_handoff,
                ),
                None => build_scenes_actor(
                    1,
                    runtime_generation,
                    event_bus.clone(),
                    events,
                    fake_settings_handle(AppSettings::default()),
                    AppSettings::default(),
                    lockout,
                ),
            };
            peers.set_peers_for_generation(1, lv1, fade);
            task.spawn();
            install_scene_document(
                &handle,
                SceneDocument {
                    scene_configs,
                    selected_scene_internal_id: None,
                },
            )
            .await;

            Self {
                handle,
                event_bus,
                snapshot,
                lv1_recalls,
                fade_commands,
            }
        }

        async fn send_recall(
            &self,
            internal_scene_id: uuid::Uuid,
        ) -> oneshot::Receiver<Result<RecallSceneResult, AppCommandError>> {
            let (reply, rx) = oneshot::channel();
            self.handle
                .send(ScenesCommand::RecallScene {
                    internal_scene_id,
                    reply,
                })
                .await
                .unwrap();
            rx
        }

        async fn next_lv1_recall(&mut self) -> ObservedLv1Recall {
            self.lv1_recalls.recv().await.expect("expected LV1 recall")
        }

        fn try_next_lv1_recall(&mut self) -> Option<ObservedLv1Recall> {
            self.lv1_recalls.try_recv().ok()
        }

        fn set_snapshot(&self, snapshot: Lv1StateSnapshot) {
            self.snapshot.send_replace(snapshot);
        }

        fn set_current_scene(&self, scene: SceneState) {
            let mut snapshot = self.snapshot.borrow().clone();
            snapshot.scene = Some(scene);
            self.snapshot.send_replace(snapshot);
        }

        fn publish_scene_observation(&self, sequence: u64, scene: SceneState) {
            self.event_bus.publish(AppEvent::Lv1 {
                generation: 1,
                event: Lv1Event::SceneChanged(SceneObservation { sequence, scene }),
            });
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    enum QueueFadeCommand {
        Recall { duration_ms: u64 },
        Wait,
        Abort,
    }

    fn queue_scene(index: i32, name: &str) -> SceneConfig {
        SceneConfig {
            internal_scene_id: uuid::Uuid::from_u128(index as u128),
            scene_index: Some(index),
            scene_name: name.to_string(),
            duration_ms: 1_000,
            channel_configs: vec![],
            scoped_channels: vec![],
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    fn queue_scene_with_fader(index: i32, name: &str, duration_ms: u64) -> SceneConfig {
        SceneConfig {
            internal_scene_id: uuid::Uuid::from_u128(index as u128),
            scene_index: Some(index),
            scene_name: name.to_string(),
            duration_ms,
            channel_configs: vec![ChannelConfig {
                group: 0,
                channel: 0,
                fader_db: Some(-12.5),
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            scoped_channels: vec![ChannelRef {
                group: 0,
                channel: 0,
            }],
            scope_toggles: SceneScopeToggles {
                faders: true,
                pan: false,
            },
        }
    }

    async fn arm_queue_recall_gate(fixture: &RecallCoordinatorFixture) {
        let baseline = SceneState {
            index: 2,
            name: "Verse".to_string(),
        };
        fixture.set_current_scene(baseline.clone());
        fixture.publish_scene_observation(1, 1, baseline);
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(30)).await;
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        yield_to_actor().await;
    }

    async fn assert_no_queue_fade_command(fixture: &mut RecallCoordinatorFixture) {
        for _ in 0..100 {
            yield_to_actor().await;
            match fixture.fade_commands.try_recv() {
                Ok(command) => panic!("unexpected Fade command: {command:?}"),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    tokio::time::advance(Duration::from_millis(1)).await;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    panic!("Fade command channel disconnected")
                }
            }
        }
    }

    async fn enqueue_in_flight_and_waiting(
        fixture: &mut RecallCoordinatorFixture,
    ) -> oneshot::Receiver<Result<RecallSceneResult, AppCommandError>> {
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        fixture.send_recall(uuid::Uuid::from_u128(2)).await
    }

    async fn enqueue_runtime_signal_in_flight_and_waiting(
        fixture: &mut RuntimeSignalFixture,
    ) -> oneshot::Receiver<Result<RecallSceneResult, AppCommandError>> {
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        fixture.send_recall(uuid::Uuid::from_u128(2)).await
    }

    async fn confirm_queue_admission(handle: &ScenesHandle) {
        let (reply, received) = oneshot::channel();
        handle
            .send(ScenesCommand::GetSessionDocument { reply })
            .await
            .unwrap();
        received.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_cancellation_pre_observation_timeout_cancels_waiting_without_aborting_fade()
     {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
        ])
        .await;
        let waiting = tokio::time::timeout(
            Duration::from_secs(1),
            enqueue_in_flight_and_waiting(&mut fixture),
        )
        .await
        .expect("first recall should dispatch");

        tokio::time::advance(RECALL_READINESS_TIMEOUT + Duration::from_millis(1)).await;
        yield_to_actor().await;

        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "LV1 recall readiness was lost"
        ));
        assert!(fixture.fade_commands.try_recv().is_err());
        assert_eq!(
            captured.matching("scene_recall_queue_cancelled", Level::WARN)[0]
                .message
                .as_deref(),
            Some("Queued scene recalls were canceled because LV1 recall readiness was lost"),
        );
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_cancellation_pre_observation_timeout_suppresses_late_exact_observation() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene_with_fader(1, "Intro", 1_000),
            queue_scene(2, "Verse"),
        ])
        .await;
        arm_queue_recall_gate(&fixture).await;
        let recall = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(recall.await.unwrap().is_ok());

        tokio::time::advance(RECALL_READINESS_TIMEOUT + Duration::from_millis(1)).await;
        yield_to_actor().await;

        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(scene.clone()),
            scene_list: vec![scene_entry(1, "Intro"), scene_entry(2, "Verse")],
            channels: vec![crate::lv1::ChannelInfo {
                group: 0,
                channel: 0,
                name: "Channel 0".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            ping_sequence: 10,
        });
        fixture.publish_scene_observation(1, 11, scene);
        tokio::time::advance(Duration::from_millis(30)).await;
        for _ in 0..10 {
            yield_to_actor().await;
        }

        assert_no_queue_fade_command(&mut fixture).await;
        assert!(fixture.try_next_lv1_recall().is_none());
    }

    #[tokio::test]
    async fn recall_queue_cancellation_matching_disconnect_cancels_waiting_but_stale_disconnect_does_not()
     {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
        ])
        .await;
        let mut waiting = enqueue_in_flight_and_waiting(&mut fixture).await;

        fixture.event_bus.publish(AppEvent::Lv1 {
            generation: 2,
            event: Lv1Event::Disconnected {
                reason: "stale".to_string(),
            },
        });
        yield_to_actor().await;
        assert!(waiting.try_recv().is_err());

        fixture.event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::Disconnected {
                reason: "lost".to_string(),
            },
        });

        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "LV1 disconnected"
        ));

        fixture.event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(vec![
                scene_entry(1, "Intro"),
                scene_entry(2, "Verse"),
            ]),
        });
        yield_to_actor().await;
        mark_runtime_peers_ready(&fixture.handle, 1).await;
        let (reply, response) = oneshot::channel();
        fixture
            .handle
            .send(ScenesCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(response.await.unwrap().ready_generation, Some(1));
    }

    #[tokio::test]
    async fn recall_queue_cancellation_active_generation_change_cancels_waiting_but_stale_fact_does_not()
     {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
        ])
        .await;
        let mut waiting = enqueue_in_flight_and_waiting(&mut fixture).await;

        fixture.event_bus.publish_runtime_generation_changed(1);
        yield_to_actor().await;
        assert!(waiting.try_recv().is_err());

        fixture.event_bus.publish_runtime_generation_changed(2);

        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "LV1 connection generation changed"
        ));
    }

    #[tokio::test]
    async fn recall_queue_cancellation_lockout_activation_cancels_waiting() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
        ])
        .await;
        let waiting = enqueue_in_flight_and_waiting(&mut fixture).await;
        confirm_queue_admission(&fixture.handle).await;
        let (reply, result) = oneshot::channel();
        fixture
            .show
            .send(crate::show::ShowCommand::SetLockout {
                enabled: true,
                reply: Some(reply),
            })
            .await
            .unwrap();
        result.await.unwrap();

        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "lockout was enabled"
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_cancellation_event_bus_lag_cancels_waiting_without_fade_abort() {
        let event_bus = AppEventBus::new(4);
        let (reached, reached_rx) = oneshot::channel();
        let (resume, resume_rx) = oneshot::channel();
        let mut fixture = RuntimeSignalFixture::connected_with_scenes(
            vec![
                queue_scene_with_fader(1, "Intro", 1_000),
                queue_scene(2, "Verse"),
            ],
            event_bus.clone(),
            event_bus.subscribe(),
            test_lockout_reader(),
            Some(BeforeFadeHandoff {
                reached,
                resume: resume_rx,
            }),
        )
        .await;

        let baseline = SceneState {
            index: 2,
            name: "Verse".to_string(),
        };
        fixture.set_current_scene(baseline.clone());
        fixture.publish_scene_observation(1, baseline);
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(30)).await;
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        yield_to_actor().await;

        let waiting = enqueue_runtime_signal_in_flight_and_waiting(&mut fixture).await;
        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(scene.clone()),
            scene_list: vec![scene_entry(1, "Intro"), scene_entry(2, "Verse")],
            channels: vec![crate::lv1::ChannelInfo {
                group: 0,
                channel: 0,
                name: "Channel 0".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            ping_sequence: 10,
        });
        fixture.publish_scene_observation(11, scene);
        tokio::time::advance(Duration::from_millis(30)).await;
        reached_rx.await.unwrap();

        event_bus.publish_lv1(1, Lv1Event::PingReceived { sequence: 11 });
        event_bus.publish(AppEvent::Settings(SettingsEvent::StateChanged {
            settings: AppSettings {
                same_scene_recall_enabled: false,
                ..Default::default()
            },
        }));
        event_bus.publish_lv1(1, Lv1Event::SceneListChanged(Vec::new()));
        event_bus.publish(AppEvent::Runtime(
            crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged {
                generation: 2,
            },
        ));
        event_bus.publish_lv1(1, Lv1Event::PingReceived { sequence: 12 });
        yield_to_actor().await;
        assert!(resume.send(()).is_err());

        let waiting_result = waiting.await.unwrap();
        assert!(
            matches!(
                &waiting_result,
                Err(AppCommandError::RecallCanceled(reason)) if reason == "LV1 recall readiness was lost"
            ),
            "unexpected waiting recall result: {waiting_result:?}"
        );
        assert!(fixture.fade_commands.try_recv().is_err());
        assert!(fixture.try_next_lv1_recall().is_none());

        let (reply, state) = oneshot::channel();
        fixture
            .handle
            .send(ScenesCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        let state = state.await.unwrap();
        assert_eq!(state.ready_generation, Some(1));
        assert_eq!(state.scene_configs[0].scene_index, Some(1));
        assert_eq!(state.scene_configs[0].scene_name, "Intro");
    }

    #[tokio::test]
    async fn recall_queue_cancellation_event_receiver_closure_cancels_waiting_without_fade_abort() {
        let event_bus = AppEventBus::default();
        let temporary_event_bus = AppEventBus::default();
        let mut fixture = RuntimeSignalFixture::connected_with_scenes(
            vec![queue_scene(1, "Intro"), queue_scene(2, "Verse")],
            event_bus,
            temporary_event_bus.subscribe(),
            test_lockout_reader(),
            None,
        )
        .await;
        let waiting = enqueue_runtime_signal_in_flight_and_waiting(&mut fixture).await;
        confirm_queue_admission(&fixture.handle).await;

        drop(temporary_event_bus);

        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "scene recall event stream closed"
        ));
        assert!(fixture.fade_commands.try_recv().is_err());
        assert!(fixture.try_next_lv1_recall().is_none());
    }

    #[tokio::test]
    async fn recall_queue_cancellation_lockout_watch_closure_cancels_waiting_without_fade_abort() {
        let event_bus = AppEventBus::default();
        let (show, show_task, show_peers, lockout) =
            crate::show::build_show_actor(event_bus.clone());
        let mut fixture = RuntimeSignalFixture::connected_with_scenes(
            vec![queue_scene(1, "Intro"), queue_scene(2, "Verse")],
            event_bus.clone(),
            event_bus.subscribe(),
            lockout,
            None,
        )
        .await;
        let waiting = enqueue_runtime_signal_in_flight_and_waiting(&mut fixture).await;
        confirm_queue_admission(&fixture.handle).await;

        drop(show);
        drop(show_peers);
        drop(show_task);

        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "lockout state is unavailable"
        ));
        assert!(fixture.fade_commands.try_recv().is_err());
        assert!(fixture.try_next_lv1_recall().is_none());
    }

    #[tokio::test]
    async fn recall_queue_cancellation_abort_all_cancels_intent_before_forwarding_one_fade_abort() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
        ])
        .await;
        let waiting = enqueue_in_flight_and_waiting(&mut fixture).await;
        let (reply, result) = oneshot::channel();
        fixture
            .handle
            .send(ScenesCommand::AbortAll { reply })
            .await
            .unwrap();

        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "Abort All was requested"
        ));
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Abort);
        assert_eq!(result.await.unwrap(), Ok(()));
        assert!(fixture.fade_commands.try_recv().is_err());
    }

    #[tokio::test]
    async fn pending_abort_fade_capacity_is_canceled_by_generation_change() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, _lv1_commands) = tokio::sync::mpsc::channel(1);
        let (fade, mut fade_commands) = tokio::sync::mpsc::channel(1);
        fade.send(FadeCommand::AbortAll { reply: None })
            .await
            .unwrap();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation.clone(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1), fade);
        task.spawn();

        let (reply, result) = oneshot::channel();
        handle
            .send(ScenesCommand::AbortAll { reply })
            .await
            .unwrap();
        yield_to_actor().await;
        runtime_generation.set(2).await;
        event_bus.publish(AppEvent::Runtime(
            crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged {
                generation: 2,
            },
        ));

        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), result)
                .await
                .expect("generation change should cancel the held abort")
                .unwrap(),
            Err(AppCommandError::StaleGeneration)
        );
        assert!(matches!(
            fade_commands.recv().await,
            Some(FadeCommand::AbortAll { reply: None })
        ));
        assert!(fade_commands.try_recv().is_err());
        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test]
    async fn pending_abort_acknowledgement_is_canceled_by_disconnect() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, _lv1_commands) = tokio::sync::mpsc::channel(1);
        let (fade, mut fade_commands) = tokio::sync::mpsc::channel(1);
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1), fade);
        task.spawn();

        let (reply, result) = oneshot::channel();
        handle
            .send(ScenesCommand::AbortAll { reply })
            .await
            .unwrap();
        let Some(FadeCommand::AbortAll {
            reply: Some(held_ack),
        }) = tokio::time::timeout(Duration::from_secs(1), fade_commands.recv())
            .await
            .expect("Scenes should send Abort All")
        else {
            panic!("expected acknowledged Fade abort");
        };

        event_bus.publish_lv1(
            1,
            Lv1Event::Disconnected {
                reason: "test disconnect".to_string(),
            },
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), result)
                .await
                .expect("disconnect should cancel the held acknowledgement")
                .unwrap(),
            Err(AppCommandError::FadeUnavailable)
        );
        assert!(held_ack.send(Ok(())).is_err());
        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn late_canceled_exact_observation_is_suppressed_once_then_allowed() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene_with_fader(1, "Intro", 1_000),
            queue_scene(2, "Verse"),
        ])
        .await;
        arm_queue_recall_gate(&fixture).await;
        let recall = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(recall.await.unwrap().is_ok());

        let (reply, result) = oneshot::channel();
        fixture
            .handle
            .send(ScenesCommand::AbortAll { reply })
            .await
            .unwrap();
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Abort);
        assert_eq!(result.await.unwrap(), Ok(()));

        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(scene.clone()),
            scene_list: vec![scene_entry(1, "Intro"), scene_entry(2, "Verse")],
            channels: vec![crate::lv1::ChannelInfo {
                group: 0,
                channel: 0,
                name: "Channel 0".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            ping_sequence: 10,
        });
        fixture.publish_scene_observation(1, 11, scene.clone());
        tokio::time::advance(Duration::from_millis(30)).await;
        assert_no_queue_fade_command(&mut fixture).await;

        fixture.publish_scene_observation(1, 12, scene);
        tokio::time::advance(Duration::from_millis(30)).await;
        assert_eq!(
            fixture.next_fade_command().await,
            QueueFadeCommand::Recall { duration_ms: 1_000 }
        );
    }

    #[tokio::test(start_paused = true)]
    async fn expired_late_canceled_observation_allows_independent_manual_recall_policy() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene_with_fader(1, "Intro", 1_000),
            queue_scene(2, "Verse"),
        ])
        .await;
        arm_queue_recall_gate(&fixture).await;
        let recall = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(recall.await.unwrap().is_ok());

        let (abort_reply, abort_result) = oneshot::channel();
        fixture
            .handle
            .send(ScenesCommand::AbortAll { reply: abort_reply })
            .await
            .unwrap();
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Abort);
        assert_eq!(abort_result.await.unwrap(), Ok(()));
        tokio::time::advance(LATE_CANCELED_OBSERVATION_TTL + Duration::from_millis(1)).await;

        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(scene.clone()),
            scene_list: vec![scene_entry(1, "Intro"), scene_entry(2, "Verse")],
            channels: vec![crate::lv1::ChannelInfo {
                group: 0,
                channel: 0,
                name: "Channel 0".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            ping_sequence: 10,
        });
        fixture.publish_scene_observation(1, 11, scene);
        tokio::time::advance(Duration::from_millis(30)).await;

        assert_eq!(
            fixture.next_fade_command().await,
            QueueFadeCommand::Recall { duration_ms: 1_000 }
        );
    }

    #[tokio::test(start_paused = true)]
    async fn current_recall_takes_precedence_while_late_cancellation_fallback_is_active() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene_with_fader(1, "Intro", 1_000),
            queue_scene(2, "Verse"),
        ])
        .await;
        arm_queue_recall_gate(&fixture).await;

        for sequence in 10..=(10 + LATE_CANCELED_OBSERVATION_CAPACITY as u64) {
            let recall = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
            let dispatch = fixture.next_lv1_recall().await;
            dispatch.reply(Ok(RecallSceneDispatch {
                scene_observation_sequence: sequence,
            }));
            assert!(recall.await.unwrap().is_ok());
            let (abort_reply, abort_result) = oneshot::channel();
            fixture
                .handle
                .send(ScenesCommand::AbortAll { reply: abort_reply })
                .await
                .unwrap();
            assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Abort);
            assert_eq!(abort_result.await.unwrap(), Ok(()));
        }

        let current = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let current_dispatch = fixture.next_lv1_recall().await;
        current_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 20,
        }));
        assert!(current.await.unwrap().is_ok());

        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(scene.clone()),
            scene_list: vec![scene_entry(1, "Intro"), scene_entry(2, "Verse")],
            channels: vec![crate::lv1::ChannelInfo {
                group: 0,
                channel: 0,
                name: "Channel 0".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            ping_sequence: 10,
        });
        fixture.publish_scene_observation(1, 21, scene);
        tokio::time::advance(Duration::from_millis(30)).await;

        assert_eq!(
            fixture.next_fade_command().await,
            QueueFadeCommand::Recall { duration_ms: 1_000 }
        );
    }

    #[tokio::test(start_paused = true)]
    async fn late_canceled_observation_overflow_suppresses_unrelated_spontaneous_recall() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene_with_fader(1, "Intro", 1_000),
            queue_scene(2, "Verse"),
            queue_scene_with_fader(3, "Chorus", 2_000),
        ])
        .await;
        arm_queue_recall_gate(&fixture).await;

        for sequence in 10..=(10 + LATE_CANCELED_OBSERVATION_CAPACITY as u64) {
            let recall = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
            let dispatch = fixture.next_lv1_recall().await;
            dispatch.reply(Ok(RecallSceneDispatch {
                scene_observation_sequence: sequence,
            }));
            assert!(recall.await.unwrap().is_ok());
            let (reply, result) = oneshot::channel();
            fixture
                .handle
                .send(ScenesCommand::AbortAll { reply })
                .await
                .unwrap();
            assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Abort);
            assert_eq!(result.await.unwrap(), Ok(()));
        }

        let unrelated = SceneState {
            index: 3,
            name: "Chorus".to_string(),
        };
        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(unrelated.clone()),
            scene_list: vec![
                scene_entry(1, "Intro"),
                scene_entry(2, "Verse"),
                scene_entry(3, "Chorus"),
            ],
            channels: vec![crate::lv1::ChannelInfo {
                group: 0,
                channel: 0,
                name: "Channel 0".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            ping_sequence: 10,
        });
        fixture.publish_scene_observation(1, 100, unrelated);
        tokio::time::advance(Duration::from_millis(30)).await;

        assert_no_queue_fade_command(&mut fixture).await;
    }

    #[tokio::test(start_paused = true)]
    async fn mismatched_late_canceled_observation_remains_eligible_for_recall_policy() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene_with_fader(1, "Intro", 1_000),
            queue_scene(2, "Verse"),
            queue_scene_with_fader(3, "Chorus", 2_000),
        ])
        .await;
        arm_queue_recall_gate(&fixture).await;

        let recall = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(recall.await.unwrap().is_ok());
        let (reply, result) = oneshot::channel();
        fixture
            .handle
            .send(ScenesCommand::AbortAll { reply })
            .await
            .unwrap();
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Abort);
        assert_eq!(result.await.unwrap(), Ok(()));

        let mismatch = SceneState {
            index: 3,
            name: "Chorus".to_string(),
        };
        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(mismatch.clone()),
            scene_list: vec![
                scene_entry(1, "Intro"),
                scene_entry(2, "Verse"),
                scene_entry(3, "Chorus"),
            ],
            channels: vec![crate::lv1::ChannelInfo {
                group: 0,
                channel: 0,
                name: "Channel 0".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            ping_sequence: 10,
        });
        fixture.publish_scene_observation(1, 11, mismatch);
        tokio::time::advance(Duration::from_millis(30)).await;

        assert_eq!(
            fixture.next_fade_command().await,
            QueueFadeCommand::Recall { duration_ms: 2_000 }
        );
    }

    #[tokio::test]
    async fn recall_queue_abort_releases_its_readiness_completion_receiver() {
        let mut fixture =
            FadeReservationLockoutFixture::connected_with_scenes(vec![queue_scene(1, "Intro")])
                .await;
        let recalled = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        fixture
            .next_lv1_recall()
            .await
            .reply(Ok(RecallSceneDispatch {
                scene_observation_sequence: 10,
            }));
        assert!(recalled.await.unwrap().is_ok());
        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_current_scene(scene.clone());
        fixture.publish_scene_observation(11, scene);
        let command = tokio::time::timeout(Duration::from_secs(1), fixture.fade_commands.recv())
            .await
            .unwrap()
            .unwrap();
        let FadeCommand::WaitForRecallReadiness {
            readiness, reply, ..
        } = command
        else {
            panic!("expected readiness without fade targets");
        };
        let completion = readiness.completion.unwrap();
        reply.unwrap().send(Ok(())).unwrap();

        let (reply, aborted) = oneshot::channel();
        fixture
            .handle
            .send(ScenesCommand::AbortAll { reply })
            .await
            .unwrap();
        let command = fixture.fade_commands.recv().await.unwrap();
        let FadeCommand::AbortAll { reply } = command else {
            panic!("expected fade abort");
        };
        reply.unwrap().send(Ok(())).unwrap();
        assert!(aborted.await.unwrap().is_ok());
        assert!(
            completion.is_closed(),
            "canceled recalls must own no pending completion task"
        );
    }

    #[tokio::test]
    async fn recall_queue_cancellation_clears_waiting_when_wait_readiness_reservation_fails() {
        let mut fixture = FadeReservationLockoutFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
        ])
        .await;
        fixture.close_fade_peer();

        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        let waiting = fixture.send_recall(uuid::Uuid::from_u128(2)).await;

        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_current_scene(scene.clone());
        fixture.publish_scene_observation(11, scene);
        tokio::time::sleep(Duration::from_millis(30)).await;

        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "Fade engine is unavailable"
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_deadline_bounds_a_stalled_fade_handoff_reply() {
        let mut fixture = FadeReservationLockoutFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
        ])
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        fixture
            .next_lv1_recall()
            .await
            .reply(Ok(RecallSceneDispatch {
                scene_observation_sequence: 10,
            }));
        assert!(first.await.unwrap().is_ok());
        let waiting = fixture.send_recall(uuid::Uuid::from_u128(2)).await;

        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_current_scene(scene.clone());
        fixture.publish_scene_observation(11, scene);
        tokio::time::advance(Duration::from_millis(30)).await;
        let stalled_command = fixture
            .fade_commands
            .recv()
            .await
            .expect("expected readiness handoff");
        assert!(matches!(
            &stalled_command,
            FadeCommand::WaitForRecallReadiness { .. }
        ));

        tokio::time::advance(RECALL_READINESS_TIMEOUT + Duration::from_millis(1)).await;
        yield_to_actor().await;
        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "LV1 recall readiness was lost"
        ));
        confirm_queue_admission(&fixture.handle).await;
        assert!(fixture.lv1_recalls.try_recv().is_err());
    }

    #[tokio::test]
    async fn recall_queue_cancellation_clears_waiting_when_recall_fade_reply_closes() {
        let mut fixture = FadeReservationLockoutFixture::connected_with_scenes(vec![
            queue_scene_with_fader(1, "Intro", 1_000),
            queue_scene(2, "Verse"),
        ])
        .await;
        let baseline = SceneState {
            index: 2,
            name: "Verse".to_string(),
        };
        fixture.set_current_scene(baseline.clone());
        fixture.publish_scene_observation(1, baseline);
        tokio::time::sleep(Duration::from_millis(30)).await;
        tokio::time::sleep(Duration::from_millis(2_050)).await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        let waiting = fixture.send_recall(uuid::Uuid::from_u128(2)).await;

        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(scene.clone()),
            scene_list: vec![scene_entry(1, "Intro"), scene_entry(2, "Verse")],
            channels: vec![crate::lv1::ChannelInfo {
                group: 0,
                channel: 0,
                name: "Channel 0".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            ping_sequence: 10,
        });
        fixture.publish_scene_observation(11, scene);
        tokio::time::sleep(Duration::from_millis(30)).await;
        let command = fixture
            .fade_commands
            .recv()
            .await
            .expect("Fade recall command should be sent");
        assert!(matches!(command, FadeCommand::RecallSceneFade { .. }));
        drop(command);

        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "Fade engine is unavailable"
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_cancels_waiting_when_lv1_peer_closes_during_completion_dispatch() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
            queue_scene(3, "Chorus"),
        ])
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let first_dispatch = fixture.next_lv1_recall().await;
        first_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        let second = fixture.send_recall(uuid::Uuid::from_u128(2)).await;
        let third = fixture.send_recall(uuid::Uuid::from_u128(3)).await;

        let recalled_scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_current_scene(recalled_scene.clone());
        fixture.publish_scene_observation(1, 11, recalled_scene);
        tokio::time::advance(Duration::from_millis(30)).await;
        yield_to_actor().await;
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Wait);

        fixture.close_lv1_peer().await;
        fixture.publish_ping(1, 11);
        fixture.publish_ping(1, 12);
        yield_to_actor().await;

        for reply in [second, third] {
            assert!(matches!(
                reply.await.unwrap(),
                Err(AppCommandError::RecallCanceled(reason)) if reason == "LV1 state is unavailable"
            ));
        }
        assert!(fixture.try_next_lv1_recall().is_none());
        assert!(fixture.fade_commands.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_skips_blocked_fresh_revalidation_and_dispatches_later_valid_recall() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
            queue_scene(3, "Chorus"),
        ])
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let first_dispatch = fixture.next_lv1_recall().await;
        first_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        let second = fixture.send_recall(uuid::Uuid::from_u128(2)).await;
        let third = fixture.send_recall(uuid::Uuid::from_u128(3)).await;

        let recalled_scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(recalled_scene.clone()),
            scene_list: vec![
                scene_entry(1, "Intro"),
                scene_entry(2, "Renamed"),
                scene_entry(3, "Chorus"),
            ],
            channels: vec![],
            ping_sequence: 10,
        });
        fixture.publish_scene_observation(1, 11, recalled_scene);
        tokio::time::advance(Duration::from_millis(30)).await;
        yield_to_actor().await;
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Wait);

        fixture.publish_ping(1, 11);
        fixture.publish_ping(1, 12);
        let third_dispatch = fixture.next_lv1_recall().await;
        assert_eq!(third_dispatch.scene_index, 3);
        third_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 12,
        }));

        assert!(matches!(
            second.await.unwrap(),
            Err(AppCommandError::CommandFailed(message)) if message == "Recall blocked: scene identity mismatch"
        ));
        assert!(third.await.unwrap().is_ok());
        assert!(fixture.fade_commands.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_cancels_later_waiting_when_lv1_recall_dispatch_fails() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
            queue_scene(3, "Chorus"),
        ])
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let first_dispatch = fixture.next_lv1_recall().await;
        first_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        let second = fixture.send_recall(uuid::Uuid::from_u128(2)).await;
        let third = fixture.send_recall(uuid::Uuid::from_u128(3)).await;

        let recalled_scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_current_scene(recalled_scene.clone());
        fixture.publish_scene_observation(1, 11, recalled_scene);
        tokio::time::advance(Duration::from_millis(30)).await;
        yield_to_actor().await;
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Wait);

        fixture.publish_ping(1, 11);
        fixture.publish_ping(1, 12);
        let second_dispatch = fixture.next_lv1_recall().await;
        assert_eq!(second_dispatch.scene_index, 2);
        second_dispatch.reply(Err(Lv1ActorError::CommandSendFailed));

        assert!(matches!(
            second.await.unwrap(),
            Err(AppCommandError::CommandFailed(message))
                if message == "LV1 actor failed to send command to LV1"
        ));
        assert!(matches!(
            third.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason))
                if reason == "LV1 recall command is unavailable"
        ));
        assert!(fixture.try_next_lv1_recall().is_none());
        assert!(fixture.fade_commands.try_recv().is_err());
    }

    #[tokio::test]
    async fn recall_queue_first_recall_dispatches_and_second_reply_stays_pending() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
        ])
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let mut second = fixture.send_recall(uuid::Uuid::from_u128(2)).await;

        let first_dispatch = fixture.next_lv1_recall().await;
        assert_eq!(first_dispatch.scene_index, 1);
        first_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert_eq!(first.await.unwrap().unwrap().lv1_scene_index, 1);

        yield_to_actor().await;
        assert!(second.try_recv().is_err());
        assert!(fixture.try_next_lv1_recall().is_none());
    }

    #[tokio::test]
    async fn recall_queue_waits_for_exact_newer_observation_and_two_later_pings() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
        ])
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let second = fixture.send_recall(uuid::Uuid::from_u128(2)).await;

        let first_dispatch = fixture.next_lv1_recall().await;
        first_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert_eq!(first.await.unwrap().unwrap().lv1_scene_index, 1);

        let exact_scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_current_scene(exact_scene.clone());
        fixture.publish_scene_observation(1, 10, exact_scene.clone());
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(fixture.try_next_lv1_recall().is_none());

        fixture.publish_scene_observation(0, 11, exact_scene.clone());
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(fixture.try_next_lv1_recall().is_none());

        let wrong_scene = SceneState {
            index: exact_scene.index,
            name: "Wrong".to_string(),
        };
        fixture.set_current_scene(wrong_scene.clone());
        fixture.publish_scene_observation(1, 11, wrong_scene);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(fixture.try_next_lv1_recall().is_none());

        fixture.set_current_scene(exact_scene.clone());
        fixture.publish_ping(1, 11);
        fixture.publish_scene_observation(1, 12, exact_scene);
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Wait);

        fixture.publish_ping(1, 12);
        yield_to_actor().await;
        assert!(fixture.try_next_lv1_recall().is_none());
        fixture.publish_ping(1, 13);

        let second_dispatch = fixture.next_lv1_recall().await;
        assert_eq!(second_dispatch.scene_index, 2);
        second_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 13,
        }));
        assert!(second.await.unwrap().is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn configured_recall_interval_blocks_next_dispatch_until_exact_boundary() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes_and_settings(
            vec![queue_scene(1, "Intro"), queue_scene(2, "Verse")],
            AppSettings {
                asc_recall_interval_ms: 2_000,
                ..Default::default()
            },
        )
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let second = fixture.send_recall(uuid::Uuid::from_u128(2)).await;
        fixture
            .next_lv1_recall()
            .await
            .reply(Ok(RecallSceneDispatch {
                scene_observation_sequence: 10,
            }));
        assert!(first.await.unwrap().is_ok());
        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_current_scene(scene.clone());
        fixture.publish_scene_observation(1, 11, scene);
        tokio::time::advance(Duration::from_millis(30)).await;
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Wait);
        fixture.publish_ping(1, 11);
        fixture.publish_ping(1, 12);
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(1_999)).await;
        yield_to_actor().await;
        assert!(fixture.try_next_lv1_recall().is_none());
        tokio::time::advance(Duration::from_millis(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        assert_eq!(dispatch.scene_index, 2);
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 12,
        }));
        assert!(second.await.unwrap().is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn recall_admitted_during_post_readiness_interval_waits_for_boundary() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes_and_settings(
            vec![queue_scene(1, "Intro"), queue_scene(2, "Verse")],
            AppSettings {
                asc_recall_interval_ms: 2_000,
                ..Default::default()
            },
        )
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        fixture
            .next_lv1_recall()
            .await
            .reply(Ok(RecallSceneDispatch {
                scene_observation_sequence: 10,
            }));
        assert!(first.await.unwrap().is_ok());
        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_current_scene(scene.clone());
        fixture.publish_scene_observation(1, 11, scene);
        tokio::time::advance(Duration::from_millis(30)).await;
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Wait);
        fixture.publish_ping(1, 11);
        fixture.publish_ping(1, 12);
        yield_to_actor().await;
        tokio::time::advance(Duration::from_secs(1)).await;
        let second = fixture.send_recall(uuid::Uuid::from_u128(2)).await;
        confirm_queue_admission(&fixture.handle).await;
        assert!(fixture.try_next_lv1_recall().is_none());
        tokio::time::advance(Duration::from_millis(999)).await;
        yield_to_actor().await;
        assert!(fixture.try_next_lv1_recall().is_none());
        tokio::time::advance(Duration::from_millis(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        assert_eq!(dispatch.scene_index, 2);
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 12,
        }));
        assert!(second.await.unwrap().is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn maximum_recall_interval_dispatches_at_the_ten_second_boundary() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes_and_settings(
            vec![queue_scene(1, "Intro"), queue_scene(2, "Verse")],
            AppSettings {
                asc_recall_interval_ms: 10_000,
                ..Default::default()
            },
        )
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let second = fixture.send_recall(uuid::Uuid::from_u128(2)).await;
        fixture
            .next_lv1_recall()
            .await
            .reply(Ok(RecallSceneDispatch {
                scene_observation_sequence: 10,
            }));
        assert!(first.await.unwrap().is_ok());
        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_current_scene(scene.clone());
        fixture.publish_scene_observation(1, 11, scene);
        tokio::time::advance(Duration::from_millis(30)).await;
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Wait);
        fixture.publish_ping(1, 11);
        fixture.publish_ping(1, 12);
        yield_to_actor().await;

        tokio::time::advance(Duration::from_millis(9_999)).await;
        yield_to_actor().await;
        assert!(fixture.try_next_lv1_recall().is_none());
        tokio::time::advance(Duration::from_millis(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        assert_eq!(dispatch.scene_index, 2);
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 12,
        }));
        assert!(second.await.unwrap().is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn ten_second_interval_does_not_extend_the_five_second_readiness_deadline() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes_and_settings(
            vec![queue_scene(1, "Intro"), queue_scene(2, "Verse")],
            AppSettings {
                asc_recall_interval_ms: 10_000,
                ..Default::default()
            },
        )
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let waiting = fixture.send_recall(uuid::Uuid::from_u128(2)).await;
        fixture
            .next_lv1_recall()
            .await
            .reply(Ok(RecallSceneDispatch {
                scene_observation_sequence: 10,
            }));
        assert!(first.await.unwrap().is_ok());

        tokio::time::advance(RECALL_READINESS_TIMEOUT + Duration::from_millis(1)).await;
        yield_to_actor().await;

        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "LV1 recall readiness was lost"
        ));
        assert!(fixture.try_next_lv1_recall().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn lockout_cancels_a_waiting_recall_and_active_post_readiness_interval() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes_and_settings(
            vec![queue_scene(1, "Intro"), queue_scene(2, "Verse")],
            AppSettings {
                asc_recall_interval_ms: 2_000,
                ..Default::default()
            },
        )
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        fixture
            .next_lv1_recall()
            .await
            .reply(Ok(RecallSceneDispatch {
                scene_observation_sequence: 10,
            }));
        assert!(first.await.unwrap().is_ok());
        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_current_scene(scene.clone());
        fixture.publish_scene_observation(1, 11, scene);
        tokio::time::advance(Duration::from_millis(30)).await;
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Wait);
        fixture.publish_ping(1, 11);
        fixture.publish_ping(1, 12);
        yield_to_actor().await;

        let waiting = fixture.send_recall(uuid::Uuid::from_u128(2)).await;
        confirm_queue_admission(&fixture.handle).await;
        let (reply, changed) = oneshot::channel();
        fixture
            .show
            .send(crate::show::ShowCommand::SetLockout {
                enabled: true,
                reply: Some(reply),
            })
            .await
            .unwrap();
        changed.await.unwrap();
        yield_to_actor().await;

        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "lockout was enabled"
        ));
        tokio::time::advance(Duration::from_secs(2)).await;
        yield_to_actor().await;
        assert!(fixture.try_next_lv1_recall().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_rechecks_lockout_immediately_before_fade_handoff() {
        let (reached, reached_rx) = oneshot::channel();
        let (resume, resume_rx) = oneshot::channel();
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes_and_handoff_gate(
            vec![
                queue_scene_with_fader(1, "Intro", 1_000),
                queue_scene(2, "Verse"),
            ],
            Some(BeforeFadeHandoff {
                reached,
                resume: resume_rx,
            }),
        )
        .await;
        arm_queue_recall_gate(&fixture).await;

        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        let waiting = fixture.send_recall(uuid::Uuid::from_u128(2)).await;
        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(scene.clone()),
            scene_list: vec![scene_entry(1, "Intro"), scene_entry(2, "Verse")],
            channels: vec![crate::lv1::ChannelInfo {
                group: 0,
                channel: 0,
                name: "Channel 0".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            ping_sequence: 10,
        });
        fixture.publish_scene_observation(1, 11, scene);
        tokio::time::advance(Duration::from_millis(30)).await;
        reached_rx.await.unwrap();

        let (lockout_reply, lockout_rx) = oneshot::channel();
        fixture
            .show
            .send(crate::show::ShowCommand::SetLockout {
                enabled: true,
                reply: Some(lockout_reply),
            })
            .await
            .unwrap();
        lockout_rx.await.unwrap();
        yield_to_actor().await;
        assert!(resume.send(()).is_err());

        assert!(fixture.fade_commands.try_recv().is_err());
        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "lockout was enabled"
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_does_not_enqueue_fade_after_lockout_during_reservation() {
        let mut fixture = FadeReservationLockoutFixture::connected_with_scenes(vec![
            queue_scene_with_fader(1, "Intro", 1_000),
            queue_scene(2, "Verse"),
        ])
        .await;
        fixture
            .fade
            .send(FadeCommand::AbortAll { reply: None })
            .await
            .unwrap();

        let baseline = SceneState {
            index: 2,
            name: "Verse".to_string(),
        };
        fixture.set_current_scene(baseline.clone());
        fixture.publish_scene_observation(1, baseline);
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(30)).await;
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        yield_to_actor().await;

        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        while fixture.state_requests.try_recv().is_ok() {}

        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_current_scene(scene.clone());
        fixture.publish_scene_observation(11, scene);
        tokio::time::advance(Duration::from_millis(30)).await;
        fixture
            .state_requests
            .recv()
            .await
            .expect("expected fresh state request before Fade reservation");
        yield_to_actor().await;

        let (lockout_reply, lockout_rx) = oneshot::channel();
        fixture
            .show
            .send(crate::show::ShowCommand::SetLockout {
                enabled: true,
                reply: Some(lockout_reply),
            })
            .await
            .unwrap();
        lockout_rx.await.unwrap();
        assert!(fixture.lockout.changed().await.unwrap());

        assert!(matches!(
            fixture.fade_commands.recv().await,
            Some(FadeCommand::AbortAll { reply: None })
        ));
        yield_to_actor().await;
        assert!(matches!(
            fixture.fade_commands.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn recall_queue_ignores_readiness_completion_after_runtime_generation_changes() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
        ])
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let _second = fixture.send_recall(uuid::Uuid::from_u128(2)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());

        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_current_scene(scene.clone());
        fixture.publish_scene_observation(1, 11, scene);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Wait);
        fixture.publish_ping(1, 11);
        fixture.runtime_generation.set(2).await;
        fixture.publish_ping(1, 12);
        yield_to_actor().await;

        assert!(fixture.try_next_lv1_recall().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_does_not_dispatch_next_recall_after_generation_changes_during_drain() {
        let event_bus = AppEventBus::default();
        let intro = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        let verse = SceneState {
            index: 2,
            name: "Verse".to_string(),
        };
        let (snapshot, snapshot_rx) = tokio::sync::watch::channel(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![scene_entry(1, "Intro"), scene_entry(2, "Verse")],
            channels: vec![],
            ping_sequence: 10,
        });
        let (recall_tx, mut recalls) = tokio::sync::mpsc::channel(8);
        let (next_snapshot_requested, next_snapshot_request) = oneshot::channel();
        let (release_snapshot, release_snapshot_rx) = oneshot::channel();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            let mut state_request_count = 0;
            let mut next_snapshot_requested = Some(next_snapshot_requested);
            let mut release_snapshot_rx = Some(release_snapshot_rx);
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    Lv1Command::GetState { reply } => {
                        state_request_count += 1;
                        // Arming, admission, initial dispatch, and exact-observation
                        // validation precede the completion-driven queue dispatch.
                        if state_request_count == 5 {
                            next_snapshot_requested
                                .take()
                                .expect("expected one completion-driven state request")
                                .send(())
                                .unwrap();
                            release_snapshot_rx
                                .take()
                                .expect("expected one held snapshot release")
                                .await
                                .unwrap();
                        }
                        let _ = reply.send(snapshot_rx.borrow().clone());
                    }
                    Lv1Command::RecallScene {
                        scene_index,
                        reply: Some(reply),
                    } => {
                        recall_tx
                            .send(ObservedLv1Recall { scene_index, reply })
                            .await
                            .unwrap();
                    }
                    _ => panic!("unexpected LV1 command"),
                }
            }
        });

        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        let (fade, mut fade_commands) = fake_queue_fade_handle(event_bus.clone());
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation.clone(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();
        install_scene_document(
            &handle,
            SceneDocument {
                scene_configs: vec![queue_scene(1, "Intro"), queue_scene(2, "Verse")],
                selected_scene_internal_id: None,
            },
        )
        .await;

        let mut armed_snapshot = snapshot.borrow().clone();
        armed_snapshot.scene = Some(verse.clone());
        snapshot.send_replace(armed_snapshot);
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(SceneObservation {
                sequence: 1,
                scene: verse,
            }),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(30)).await;
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        yield_to_actor().await;

        let (first_reply, first) = oneshot::channel();
        handle
            .send(ScenesCommand::RecallScene {
                internal_scene_id: uuid::Uuid::from_u128(1),
                reply: first_reply,
            })
            .await
            .unwrap();
        let first_dispatch = recalls.recv().await.unwrap();
        assert_eq!(first_dispatch.scene_index, 1);
        first_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        let (second_reply, _second) = oneshot::channel();
        handle
            .send(ScenesCommand::RecallScene {
                internal_scene_id: uuid::Uuid::from_u128(2),
                reply: second_reply,
            })
            .await
            .unwrap();

        let mut recalled_snapshot = snapshot.borrow().clone();
        recalled_snapshot.scene = Some(intro.clone());
        snapshot.send_replace(recalled_snapshot);
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(SceneObservation {
                sequence: 11,
                scene: intro,
            }),
        });
        tokio::time::advance(Duration::from_millis(30)).await;
        assert_eq!(fade_commands.recv().await, Some(QueueFadeCommand::Wait));

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::PingReceived { sequence: 11 },
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::PingReceived { sequence: 12 },
        });
        next_snapshot_request
            .await
            .expect("matching readiness completion should begin the next queue drain");

        runtime_generation.set(2).await;
        release_snapshot.send(()).unwrap();
        yield_to_actor().await;
        assert!(matches!(
            recalls.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn readiness_failure_cancels_a_pending_recall_admission() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, mut lv1_commands) = tokio::sync::mpsc::channel(8);
        let (fade, mut fade_commands) = tokio::sync::mpsc::channel(1);
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1), fade);
        task.spawn();
        install_scene_document(
            &handle,
            SceneDocument {
                scene_configs: vec![queue_scene(1, "Intro"), queue_scene(2, "Verse")],
                selected_scene_internal_id: None,
            },
        )
        .await;
        let snapshot = |scene| Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene,
            scene_list: vec![scene_entry(1, "Intro"), scene_entry(2, "Verse")],
            channels: Vec::new(),
            ping_sequence: 10,
        };

        let (first_reply, first_result) = oneshot::channel();
        handle
            .send(ScenesCommand::RecallScene {
                internal_scene_id: uuid::Uuid::from_u128(1),
                reply: first_reply,
            })
            .await
            .unwrap();
        let Some(Lv1Command::GetState { reply }) = lv1_commands.recv().await else {
            panic!("expected first admission snapshot");
        };
        reply.send(snapshot(None)).unwrap();
        let Some(Lv1Command::GetState { reply }) = lv1_commands.recv().await else {
            panic!("expected first dispatch snapshot");
        };
        reply.send(snapshot(None)).unwrap();
        let Some(Lv1Command::RecallScene {
            reply: Some(dispatch_reply),
            ..
        }) = lv1_commands.recv().await
        else {
            panic!("expected first LV1 recall");
        };
        dispatch_reply
            .send(Ok(RecallSceneDispatch {
                scene_observation_sequence: 10,
            }))
            .unwrap();
        assert!(first_result.await.unwrap().is_ok());

        let intro = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        event_bus.publish_lv1(
            1,
            Lv1Event::SceneChanged(SceneObservation {
                sequence: 11,
                scene: intro.clone(),
            }),
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
        let Some(Lv1Command::GetState { reply }) = lv1_commands.recv().await else {
            panic!("expected exact observation snapshot");
        };
        reply.send(snapshot(Some(intro))).unwrap();
        let Some(FadeCommand::WaitForRecallReadiness {
            readiness,
            reply: Some(fade_reply),
            ..
        }) = fade_commands.recv().await
        else {
            panic!("expected readiness-only Fade handoff");
        };
        let readiness_completion = readiness
            .completion
            .expect("queue owns readiness completion");
        fade_reply.send(Ok(())).unwrap();
        yield_to_actor().await;

        let (second_reply, second_result) = oneshot::channel();
        handle
            .send(ScenesCommand::RecallScene {
                internal_scene_id: uuid::Uuid::from_u128(2),
                reply: second_reply,
            })
            .await
            .unwrap();
        let Some(Lv1Command::GetState {
            reply: held_admission,
        }) = lv1_commands.recv().await
        else {
            panic!("expected held second admission snapshot");
        };

        readiness_completion
            .send(Err(RecallReadinessError::TimedOut {
                generation: 1,
                scene_index: 1,
                scene_name: "Intro".to_string(),
                observed_ping_count: 0,
            }))
            .unwrap();

        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(1), second_result)
                .await
                .expect("readiness failure should cancel the held admission")
                .unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "LV1 recall readiness was lost"
        ));
        assert!(held_admission.send(snapshot(None)).is_err());
        assert!(lv1_commands.try_recv().is_err());
        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test]
    async fn pending_recall_admission_snapshot_is_canceled_by_disconnect() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, mut lv1_commands) = tokio::sync::mpsc::channel(8);
        let (fade, _fade_commands) = tokio::sync::mpsc::channel(1);
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1), fade);
        task.spawn();
        install_scene_document(
            &handle,
            SceneDocument {
                scene_configs: vec![queue_scene(1, "Intro")],
                selected_scene_internal_id: None,
            },
        )
        .await;

        let (reply, result) = oneshot::channel();
        handle
            .send(ScenesCommand::RecallScene {
                internal_scene_id: uuid::Uuid::from_u128(1),
                reply,
            })
            .await
            .unwrap();
        let Some(Lv1Command::GetState {
            reply: held_snapshot,
        }) = lv1_commands.recv().await
        else {
            panic!("expected held admission snapshot");
        };

        event_bus.publish_lv1(
            1,
            Lv1Event::Disconnected {
                reason: "test disconnect".to_string(),
            },
        );
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(1), result)
                .await
                .expect("disconnect should cancel the held admission snapshot")
                .unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "LV1 disconnected"
        ));
        assert!(
            held_snapshot
                .send(Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: vec![scene_entry(1, "Intro")],
                    channels: Vec::new(),
                    ping_sequence: 10,
                })
                .is_err()
        );
        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_does_not_dispatch_initial_recall_after_generation_changes_during_dispatch()
     {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let event_bus = AppEventBus::default();
        let (snapshot_request, snapshot_requested) = oneshot::channel();
        let (release_snapshot, release_snapshot_rx) = oneshot::channel();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            let mut state_request_count = 0;
            let mut snapshot_request = Some(snapshot_request);
            let mut release_snapshot_rx = Some(release_snapshot_rx);
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    Lv1Command::GetState { reply } => {
                        state_request_count += 1;
                        if state_request_count == 2 {
                            snapshot_request
                                .take()
                                .expect("expected dispatch-time state request")
                                .send(())
                                .unwrap();
                            release_snapshot_rx
                                .take()
                                .expect("expected held snapshot release")
                                .await
                                .unwrap();
                        }
                        let _ = reply.send(Lv1StateSnapshot {
                            connection: ConnectionStatus::Connected,
                            scene: None,
                            scene_list: vec![scene_entry(1, "Intro")],
                            channels: vec![],
                            ping_sequence: 10,
                        });
                    }
                    Lv1Command::RecallScene { .. } => {
                        panic!("stale initial dispatch must not reach LV1")
                    }
                    _ => panic!("unexpected LV1 command"),
                }
            }
        });

        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        let (fade, mut fade_commands) = fake_queue_fade_handle(event_bus.clone());
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation.clone(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();
        install_scene_document(
            &handle,
            SceneDocument {
                scene_configs: vec![queue_scene(1, "Intro")],
                selected_scene_internal_id: None,
            },
        )
        .await;

        let (reply, recall) = oneshot::channel();
        handle
            .send(ScenesCommand::RecallScene {
                internal_scene_id: uuid::Uuid::from_u128(1),
                reply,
            })
            .await
            .unwrap();
        snapshot_requested
            .await
            .expect("initial dispatch should await fresh state");
        runtime_generation.set(2).await;
        event_bus.publish(AppEvent::Runtime(
            crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged {
                generation: 2,
            },
        ));
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(1), recall)
                .await
                .expect("generation change should cancel the held dispatch snapshot")
                .unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "LV1 connection generation changed"
        ));
        release_snapshot.send(()).unwrap();
        assert!(fade_commands.try_recv().is_err());
        assert!(
            captured
                .matching("scene_recall_blocked", Level::WARN)
                .is_empty()
        );
    }

    #[tokio::test]
    async fn pending_recall_dispatch_reply_is_canceled_by_disconnect() {
        let snapshot = || Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![scene_entry(1, "Intro")],
            channels: Vec::new(),
            ping_sequence: 10,
        };
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, mut lv1_commands) = tokio::sync::mpsc::channel(8);
        let (fade, _fade_commands) = tokio::sync::mpsc::channel(1);
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1), fade);
        task.spawn();
        install_scene_document(
            &handle,
            SceneDocument {
                scene_configs: vec![queue_scene(1, "Intro")],
                selected_scene_internal_id: None,
            },
        )
        .await;

        let (reply, result) = oneshot::channel();
        handle
            .send(ScenesCommand::RecallScene {
                internal_scene_id: uuid::Uuid::from_u128(1),
                reply,
            })
            .await
            .unwrap();
        for _ in 0..2 {
            let Some(Lv1Command::GetState { reply }) = lv1_commands.recv().await else {
                panic!("expected admission and dispatch snapshots");
            };
            reply.send(snapshot()).unwrap();
        }
        let Some(Lv1Command::RecallScene {
            reply: Some(held_reply),
            ..
        }) = lv1_commands.recv().await
        else {
            panic!("expected held LV1 recall acknowledgement");
        };

        event_bus.publish_lv1(
            1,
            Lv1Event::Disconnected {
                reason: "test disconnect".to_string(),
            },
        );
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(1), result)
                .await
                .expect("disconnect should cancel the held recall dispatch")
                .unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "LV1 disconnected"
        ));
        assert!(
            held_reply
                .send(Ok(RecallSceneDispatch {
                    scene_observation_sequence: 11,
                }))
                .is_err()
        );
        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test]
    async fn recall_queue_keeps_repeated_same_scene_requests_distinct() {
        let mut fixture =
            RecallCoordinatorFixture::connected_with_scenes(vec![queue_scene(1, "Intro")]).await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let second = fixture.send_recall(uuid::Uuid::from_u128(1)).await;

        let first_dispatch = fixture.next_lv1_recall().await;
        first_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());

        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_current_scene(scene.clone());
        fixture.publish_scene_observation(1, 11, scene.clone());
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Wait);
        fixture.publish_ping(1, 11);
        fixture.publish_ping(1, 12);

        let second_dispatch = fixture.next_lv1_recall().await;
        second_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 12,
        }));
        assert!(second.await.unwrap().is_ok());

        fixture.publish_scene_observation(1, 13, scene);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Wait);
    }

    #[tokio::test]
    async fn recall_queue_waits_for_readiness_when_scene_scope_is_disabled() {
        let mut disabled = queue_scene(1, "Intro");
        disabled.scoped_channels = vec![ChannelRef {
            group: 0,
            channel: 0,
        }];
        disabled.channel_configs = vec![ChannelConfig {
            group: 0,
            channel: 0,
            fader_db: Some(-12.5),
            pan: None,
            balance: None,
            width: None,
            pan_mode: None,
        }];
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            disabled,
            queue_scene(2, "Verse"),
        ])
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let second = fixture.send_recall(uuid::Uuid::from_u128(2)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());

        let scene = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(scene.clone()),
            scene_list: vec![scene_entry(1, "Intro"), scene_entry(2, "Verse")],
            channels: vec![crate::lv1::ChannelInfo {
                group: 0,
                channel: 0,
                name: "Channel 0".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            ping_sequence: 10,
        });
        fixture.publish_scene_observation(1, 11, scene);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(fixture.next_fade_command().await, QueueFadeCommand::Wait);
        fixture.publish_ping(1, 11);
        yield_to_actor().await;
        assert!(fixture.try_next_lv1_recall().is_none());
        fixture.publish_ping(1, 12);
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 12,
        }));
        assert!(second.await.unwrap().is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_zero_duration_start_writes_immediately_and_waits_for_readiness() {
        let event_bus = AppEventBus::default();
        let (show, show_task, _show_peers, lockout) =
            crate::show::build_show_actor(event_bus.clone());
        show_task.spawn();
        let intro = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        let verse = SceneState {
            index: 2,
            name: "Verse".to_string(),
        };
        let snapshot = Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(verse.clone()),
            scene_list: vec![scene_entry(1, "Intro"), scene_entry(2, "Verse")],
            channels: vec![crate::lv1::ChannelInfo {
                group: 0,
                channel: 0,
                name: "Channel 0".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            ping_sequence: 10,
        };
        let (snapshot_tx, snapshot_rx) = tokio::sync::watch::channel(snapshot);
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        let (recall_tx, mut recalls) = tokio::sync::mpsc::channel(8);
        let (write_tx, mut writes) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    Lv1Command::GetState { reply } => {
                        let _ = reply.send(snapshot_rx.borrow().clone());
                    }
                    Lv1Command::RecallScene {
                        scene_index,
                        reply: Some(reply),
                    } => {
                        recall_tx
                            .send(ObservedLv1Recall { scene_index, reply })
                            .await
                            .unwrap();
                    }
                    Lv1Command::WriteBatch(writes) => {
                        let _ = write_tx.send(writes).await;
                    }
                    _ => panic!("unexpected LV1 command"),
                }
            }
        });

        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        let (real_fade, fade_task) = crate::fade::build_engine(
            runtime_generation.clone(),
            event_bus.clone(),
            1,
            lv1.clone(),
        );
        fade_task.spawn();
        let (fade_tx, mut fade_rx) = tokio::sync::mpsc::channel(8);
        let fade_proxy = fade_tx;
        let real_fade_for_proxy = real_fade.clone();
        let (seen_tx, mut seen) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(command) = fade_rx.recv().await {
                if let FadeCommand::RecallSceneFade { config, .. } = &command {
                    let _ = seen_tx
                        .send(QueueFadeCommand::Recall {
                            duration_ms: config.duration_ms,
                        })
                        .await;
                }
                real_fade_for_proxy.send(command).await.unwrap();
            }
        });
        let (scenes, scenes_task, scenes_peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            lockout,
        );
        scenes_peers.set_peers_for_generation(1, lv1, fade_proxy);
        scenes_task.spawn();
        install_scene_document(
            &scenes,
            SceneDocument {
                scene_configs: vec![
                    queue_scene_with_fader(1, "Intro", 0),
                    queue_scene(2, "Verse"),
                ],
                selected_scene_internal_id: None,
            },
        )
        .await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(SceneObservation {
                sequence: 1,
                scene: verse,
            }),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(30)).await;
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        yield_to_actor().await;

        let (first_reply, first) = oneshot::channel();
        scenes
            .send(ScenesCommand::RecallScene {
                internal_scene_id: uuid::Uuid::from_u128(1),
                reply: first_reply,
            })
            .await
            .unwrap();
        let first_dispatch = recalls.recv().await.unwrap();
        assert_eq!(first_dispatch.scene_index, 1);
        first_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        let (second_reply, _second) = oneshot::channel();
        scenes
            .send(ScenesCommand::RecallScene {
                internal_scene_id: uuid::Uuid::from_u128(2),
                reply: second_reply,
            })
            .await
            .unwrap();

        let mut recalled_snapshot = snapshot_tx.borrow().clone();
        recalled_snapshot.scene = Some(intro.clone());
        snapshot_tx.send_replace(recalled_snapshot);
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(SceneObservation {
                sequence: 11,
                scene: intro,
            }),
        });
        tokio::time::advance(Duration::from_millis(30)).await;
        yield_to_actor().await;

        assert_eq!(
            seen.recv().await.unwrap(),
            QueueFadeCommand::Recall { duration_ms: 0 }
        );
        assert_eq!(writes.recv().await.unwrap()[0].value, -12.5);

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::PingReceived { sequence: 11 },
        });
        yield_to_actor().await;
        assert!(recalls.try_recv().is_err());
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::PingReceived { sequence: 12 },
        });
        assert_eq!(recalls.recv().await.unwrap().scene_index, 2);
        drop(show);
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_cancels_waiting_recall_when_real_fade_readiness_times_out() {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let event_bus = AppEventBus::default();
        let mut fade_events = event_bus.subscribe();
        let (show, show_task, _show_peers, lockout) =
            crate::show::build_show_actor(event_bus.clone());
        show_task.spawn();
        let intro = SceneState {
            index: 1,
            name: "Intro".to_string(),
        };
        let verse = SceneState {
            index: 2,
            name: "Verse".to_string(),
        };
        let snapshot = Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(verse.clone()),
            scene_list: vec![scene_entry(1, "Intro"), scene_entry(2, "Verse")],
            channels: vec![crate::lv1::ChannelInfo {
                group: 0,
                channel: 0,
                name: "Channel 0".to_string(),
                gain_db: 0.0,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            ping_sequence: 10,
        };
        let (snapshot_tx, snapshot_rx) = tokio::sync::watch::channel(snapshot);
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        let (recall_tx, mut recalls) = tokio::sync::mpsc::channel(8);
        let (write_tx, mut writes) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    Lv1Command::GetState { reply } => {
                        let _ = reply.send(snapshot_rx.borrow().clone());
                    }
                    Lv1Command::RecallScene {
                        scene_index,
                        reply: Some(reply),
                    } => {
                        recall_tx
                            .send(ObservedLv1Recall { scene_index, reply })
                            .await
                            .unwrap();
                    }
                    Lv1Command::WriteBatch(writes) => {
                        let _ = write_tx.send(writes).await;
                    }
                    _ => panic!("unexpected LV1 command"),
                }
            }
        });

        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        let (real_fade, fade_task) = crate::fade::build_engine(
            runtime_generation.clone(),
            event_bus.clone(),
            1,
            lv1.clone(),
        );
        fade_task.spawn();
        let (fade_tx, mut fade_rx) = tokio::sync::mpsc::channel(8);
        let fade_proxy = fade_tx;
        let real_fade_for_proxy = real_fade.clone();
        let (seen_tx, mut seen) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(command) = fade_rx.recv().await {
                let observed = match &command {
                    FadeCommand::RecallSceneFade { config, .. } => Some(QueueFadeCommand::Recall {
                        duration_ms: config.duration_ms,
                    }),
                    _ => None,
                };
                real_fade_for_proxy.send(command).await.unwrap();
                if let Some(observed) = observed {
                    let _ = seen_tx.send(observed).await;
                }
            }
        });
        let (scenes, scenes_task, scenes_peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            lockout,
        );
        scenes_peers.set_peers_for_generation(1, lv1, fade_proxy);
        scenes_task.spawn();
        install_scene_document(
            &scenes,
            SceneDocument {
                scene_configs: vec![
                    queue_scene_with_fader(1, "Intro", 1_000),
                    queue_scene(2, "Verse"),
                ],
                selected_scene_internal_id: None,
            },
        )
        .await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(SceneObservation {
                sequence: 1,
                scene: verse,
            }),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(30)).await;
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        yield_to_actor().await;

        let (first_reply, first) = oneshot::channel();
        scenes
            .send(ScenesCommand::RecallScene {
                internal_scene_id: uuid::Uuid::from_u128(1),
                reply: first_reply,
            })
            .await
            .unwrap();
        let first_dispatch = recalls.recv().await.unwrap();
        assert_eq!(first_dispatch.scene_index, 1);
        let deadline = tokio::time::Instant::now() + RECALL_READINESS_TIMEOUT;
        first_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        let (waiting_reply, waiting) = oneshot::channel();
        scenes
            .send(ScenesCommand::RecallScene {
                internal_scene_id: uuid::Uuid::from_u128(2),
                reply: waiting_reply,
            })
            .await
            .unwrap();

        let mut recalled_snapshot = snapshot_tx.borrow().clone();
        recalled_snapshot.scene = Some(intro.clone());
        snapshot_tx.send_replace(recalled_snapshot);
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(SceneObservation {
                sequence: 11,
                scene: intro,
            }),
        });
        tokio::time::advance(Duration::from_millis(30)).await;
        yield_to_actor().await;

        assert_eq!(
            seen.recv().await.unwrap(),
            QueueFadeCommand::Recall { duration_ms: 1_000 }
        );
        yield_to_actor().await;
        assert_eq!(
            captured
                .matching("fade_post_recall_ping_barrier", Level::DEBUG)
                .len(),
            1
        );
        assert!(writes.try_recv().is_err());
        scenes_peers.clear_peers_for_generation(1);

        tokio::time::advance(deadline.duration_since(tokio::time::Instant::now())).await;
        yield_to_actor().await;
        loop {
            if let AppEvent::Fade {
                generation: 1,
                event: crate::fade::FadeEvent::FadeAborted,
            } = fade_events.recv().await.unwrap()
            {
                break;
            }
        }
        assert!(matches!(
            waiting.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "LV1 recall readiness was lost"
        ));
        yield_to_actor().await;
        assert!(recalls.try_recv().is_err());

        let cancellation_warnings = captured.matching("scene_recall_queue_cancelled", Level::WARN);
        assert_eq!(cancellation_warnings.len(), 1);
        assert_eq!(
            cancellation_warnings[0].message.as_deref(),
            Some(
                "Paused fades were aborted and queued scene recalls were canceled because LV1 did not resume its keepalive cadence after scene recall"
            )
        );
        assert!(
            captured
                .matching("fade_post_recall_ping_timeout", Level::WARN)
                .is_empty()
        );
        drop(show);
    }

    #[tokio::test]
    async fn recall_queue_shutdown_cancels_waiting_recall_without_rejecting_in_flight_reply() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
        ])
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        let second = fixture.send_recall(uuid::Uuid::from_u128(2)).await;

        fixture.handle.send(ScenesCommand::Shutdown).await.unwrap();

        assert!(matches!(
            second.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "Scenes actor stopped"
        ));
        assert!(fixture.try_next_lv1_recall().is_none());
    }

    #[tokio::test]
    async fn recall_queue_command_channel_closure_cancels_waiting_recall() {
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
        ])
        .await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        let second = fixture.send_recall(uuid::Uuid::from_u128(2)).await;
        confirm_queue_admission(&fixture.handle).await;
        drop(fixture.handle);

        assert!(matches!(
            second.await.unwrap(),
            Err(AppCommandError::RecallCanceled(reason)) if reason == "Scenes actor stopped"
        ));
    }

    #[tokio::test]
    async fn recall_queue_capacity_counts_the_in_flight_recall() {
        let scenes = (1..=9)
            .map(|index| queue_scene(index, &format!("Scene {index}")))
            .collect();
        let mut fixture = RecallCoordinatorFixture::connected_with_scenes(scenes).await;
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let first_dispatch = fixture.next_lv1_recall().await;
        first_dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());

        let mut waiting = Vec::new();
        for index in 2..=8 {
            waiting.push(fixture.send_recall(uuid::Uuid::from_u128(index)).await);
        }
        let ninth = fixture.send_recall(uuid::Uuid::from_u128(9)).await;

        yield_to_actor().await;
        assert_eq!(ninth.await.unwrap(), Err(AppCommandError::RecallQueueFull));
        for reply in &mut waiting {
            assert!(reply.try_recv().is_err());
        }
        assert!(fixture.try_next_lv1_recall().is_none());
    }

    #[tokio::test]
    async fn recall_queue_rejects_invalid_requests_without_admission_or_dispatch() {
        let mut fixture =
            RecallCoordinatorFixture::connected_with_scenes(vec![queue_scene(1, "Intro")]).await;

        let (lockout_reply, lockout_rx) = oneshot::channel();
        fixture
            .show
            .send(crate::show::ShowCommand::SetLockout {
                enabled: true,
                reply: Some(lockout_reply),
            })
            .await
            .unwrap();
        lockout_rx.await.unwrap();
        let lockout = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        assert!(matches!(
            lockout.await.unwrap(),
            Err(AppCommandError::CommandFailed(message)) if message == "Recall blocked: lockout is enabled"
        ));

        let (unlock_reply, unlock_rx) = oneshot::channel();
        fixture
            .show
            .send(crate::show::ShowCommand::SetLockout {
                enabled: false,
                reply: Some(unlock_reply),
            })
            .await
            .unwrap();
        unlock_rx.await.unwrap();

        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Disconnected,
            scene: None,
            scene_list: vec![scene_entry(1, "Intro")],
            channels: vec![],
            ping_sequence: 10,
        });
        let disconnected = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        assert!(matches!(
            disconnected.await.unwrap(),
            Err(AppCommandError::CommandFailed(message)) if message == "Recall blocked: LV1 is disconnected"
        ));

        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![scene_entry(1, "Renamed")],
            channels: vec![],
            ping_sequence: 10,
        });
        let missing = fixture.send_recall(uuid::Uuid::from_u128(2)).await;
        assert!(matches!(
            missing.await.unwrap(),
            Err(AppCommandError::CommandFailed(message)) if message == "Scene config not found"
        ));

        let mismatch = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        assert!(matches!(
            mismatch.await.unwrap(),
            Err(AppCommandError::CommandFailed(message)) if message == "Recall blocked: scene identity mismatch"
        ));

        assert!(fixture.try_next_lv1_recall().is_none());
    }

    #[test]
    fn connected_empty_snapshot_is_authoritative_but_disconnected_list_is_not() {
        let connected = Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
            ping_sequence: 0,
        };
        assert_eq!(authoritative_scene_list(&connected), Some(Vec::new()));

        let disconnected = Lv1StateSnapshot {
            connection: ConnectionStatus::Disconnected,
            scene: None,
            scene_list: vec![scene_entry(1, "Stale")],
            channels: Vec::new(),
            ping_sequence: 0,
        };
        assert_eq!(authoritative_scene_list(&disconnected), None);
    }

    async fn arm_recall_state(event_bus: &AppEventBus) {
        arm_recall_state_for_generation(event_bus, 1).await;
    }

    async fn arm_recall_state_for_generation(event_bus: &AppEventBus, generation: u64) {
        event_bus.publish(AppEvent::Lv1 {
            generation,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(550)).await;
        yield_to_actor().await;
    }

    async fn yield_to_actor() {
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
    }

    fn song_3_at(index: i32) -> SceneObservation {
        SceneObservation {
            sequence: 1,
            scene: SceneState {
                index,
                name: "Song 3".to_string(),
            },
        }
    }

    fn scene_entry(index: i32, name: &str) -> SceneListEntry {
        SceneListEntry {
            index,
            name: name.to_string(),
        }
    }

    fn scene_list_before_current_move() -> Vec<SceneListEntry> {
        vec![
            scene_entry(0, "My first scene"),
            scene_entry(1, "Song 1"),
            scene_entry(2, "My second scene"),
            scene_entry(3, "Song 2 -- Changed"),
            scene_entry(4, "Song 3"),
            scene_entry(5, "Test"),
        ]
    }

    fn scene_list_after_current_move() -> Vec<SceneListEntry> {
        vec![
            scene_entry(0, "My first scene"),
            scene_entry(1, "Song 1"),
            scene_entry(2, "My second scene"),
            scene_entry(3, "Song 3"),
            scene_entry(4, "Song 2 -- Changed"),
            scene_entry(5, "Test"),
        ]
    }

    fn scene_list_before_non_current_rename() -> Vec<SceneListEntry> {
        vec![
            scene_entry(0, "My first scene"),
            scene_entry(1, "Song 1"),
            scene_entry(2, "My second scene"),
            scene_entry(3, "Song 2"),
            scene_entry(4, "Song 3"),
            scene_entry(5, "Test"),
        ]
    }

    fn scene_list_after_non_current_rename() -> Vec<SceneListEntry> {
        vec![
            scene_entry(0, "My first scene"),
            scene_entry(1, "Song 1"),
            scene_entry(2, "My second scene"),
            scene_entry(3, "Song 2 -- Changed"),
            scene_entry(4, "Song 3"),
            scene_entry(5, "Test"),
        ]
    }

    #[tokio::test(start_paused = true)]
    async fn scene_list_changed_publishes_default_scene_configs() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1_tx, _lv1_rx) = tokio::sync::mpsc::channel(1);
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();
        mark_runtime_peers_ready(&handle, 1).await;
        while events.try_recv().is_ok() {}

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(vec![
                scene_entry(0, "Smoke A"),
                scene_entry(1, "Smoke B"),
            ]),
        });

        let state = loop {
            if let AppEvent::Scenes {
                generation: 1,
                event:
                    ScenesEvent::StateChanged {
                        state,
                        persisted_scene_edit,
                        ..
                    },
            } = events.recv().await.unwrap()
            {
                assert!(persisted_scene_edit);
                break state;
            }
        };
        assert_eq!(state.scene_configs.len(), 2);
        assert_eq!(state.scene_configs[0].scene_index, Some(0));
        assert_eq!(state.scene_configs[0].scene_name, "Smoke A");
        assert_eq!(state.scene_configs[1].scene_index, Some(1));
        assert_eq!(state.scene_configs[1].scene_name, "Smoke B");
        for config in &state.scene_configs {
            assert!(config.channel_configs.is_empty());
            assert!(config.scoped_channels.is_empty());
            assert_eq!(config.scope_toggles, SceneScopeToggles::default());
        }

        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn stale_scene_list_is_ignored_until_current_generation_is_ready() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1_tx, _lv1_rx) = tokio::sync::mpsc::channel(1);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1_tx), fade);
        task.spawn();
        mark_runtime_peers_ready(&handle, 1).await;
        while events.try_recv().is_ok() {}

        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneListChanged(vec![scene_entry(1, "Stale")]),
        });
        tokio::task::yield_now().await;
        while let Ok(event) = events.try_recv() {
            assert!(!matches!(event, AppEvent::Scenes { .. }));
        }

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(vec![scene_entry(1, "Current")]),
        });
        let state = next_scene_state_changed_for_generation(&mut events, 1).await;
        assert_eq!(state.ready_generation, Some(1));
        assert_eq!(state.scene_configs[0].scene_name, "Current");

        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test]
    async fn reconnect_waits_for_authoritative_scene_list_before_realigning_configs() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1_tx, _lv1_rx) = tokio::sync::mpsc::channel(1);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation.clone(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1_tx), fade);
        task.spawn();
        let document = intro_scene_document();
        let scene_id = document.scene_configs[0].internal_scene_id;
        crate::session::tests::replace_scenes(&handle, document, 1).await;
        mark_runtime_peers_ready_with_list(&handle, 1, vec![scene_entry(1, "Intro")]).await;
        while events.try_recv().is_ok() {}

        runtime_generation.set(2).await;
        event_bus.publish_runtime_generation_changed(2);
        let disconnected = next_scene_state_changed_for_generation(&mut events, 2).await;
        assert_eq!(disconnected.ready_generation, None);
        assert_eq!(disconnected.scene_configs.len(), 1);
        assert_eq!(disconnected.scene_configs[0].scene_index, Some(1));

        let (lv1_tx, _lv1_rx) = tokio::sync::mpsc::channel(1);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        peers.set_peers_for_generation(2, crate::lv1::test_actor_handle(lv1_tx), fade);
        mark_runtime_peers_ready_with_list(&handle, 2, Vec::new()).await;

        let (reply, state) = oneshot::channel();
        handle
            .send(ScenesCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        let awaiting_scene_list = state.await.unwrap();
        assert_eq!(awaiting_scene_list.ready_generation, None);
        assert_eq!(awaiting_scene_list.scene_configs.len(), 1);
        assert_eq!(
            awaiting_scene_list.scene_configs[0].internal_scene_id,
            scene_id
        );
        assert_eq!(awaiting_scene_list.scene_configs[0].scene_index, Some(1));

        while events.try_recv().is_ok() {}
        event_bus.publish_lv1(2, Lv1Event::SceneListChanged(vec![scene_entry(1, "Intro")]));
        let reconnected = next_scene_state_changed_for_generation(&mut events, 2).await;
        assert_eq!(reconnected.ready_generation, Some(2));
        assert_eq!(reconnected.scene_configs.len(), 1);
        assert_eq!(reconnected.scene_configs[0].internal_scene_id, scene_id);
        assert_eq!(reconnected.scene_configs[0].scene_index, Some(1));

        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn scene_list_changed_updates_existing_scene_configs() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1_tx, _lv1_rx) = tokio::sync::mpsc::channel(1);
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let handle = build_and_spawn_scene_recall_fader_with_document(
            1,
            runtime_generation,
            lv1,
            fade,
            event_bus.clone(),
            intro_scene_document(),
        )
        .await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(vec![scene_entry(1, "Intro Renamed")]),
        });

        let state =
            next_scene_state_with_name_for_generation(&mut events, "Intro Renamed", 1).await;
        assert_eq!(state.scene_configs.len(), 1);
        assert_eq!(
            state.scene_configs[0].internal_scene_id,
            intro_internal_scene_id()
        );
        assert_eq!(state.scene_configs[0].scene_index, Some(1));
        assert_eq!(state.scene_configs[0].scene_name, "Intro Renamed");

        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn scene_list_alignment_logs_diagnostic_when_configs_change() {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1_tx, _lv1_rx) = tokio::sync::mpsc::channel(1);
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let handle = build_and_spawn_scene_recall_fader_with_document(
            1,
            runtime_generation,
            lv1,
            fade,
            event_bus.clone(),
            intro_scene_document(),
        )
        .await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(vec![scene_entry(1, "Intro Renamed")]),
        });
        let _ = next_scene_state_with_name_for_generation(&mut events, "Intro Renamed", 1).await;

        assert!(
            captured
                .events()
                .iter()
                .any(|log| log.event.as_deref() == Some("session_scene_alignment"))
        );

        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn unavailable_lv1_state_blocks_before_start() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1_tx, lv1_rx) = tokio::sync::mpsc::channel(1);
        drop(lv1_rx);
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            lv1,
            fade,
            event_bus.clone(),
        )
        .await;
        arm_recall_state(&event_bus).await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });

        match next_scene_recall_event_for_generation(&mut events, 1).await {
            ScenesEvent::Blocked { reason, .. } => {
                assert!(reason.contains("LV1 state is unavailable"));
            }
            other => panic!("unexpected event: {other:?}"),
        }

        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn blocked_recall_does_not_start_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade_tx, fade_rx) = tokio::sync::mpsc::channel(1);
        drop(fade_rx);
        let fade = fade_tx;

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            lv1,
            fade,
            event_bus.clone(),
        )
        .await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;

        assert!(next_blocked_scene_recall_event_for_generation(&mut events, 1).await);

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn explicit_recall_rechecks_lockout_after_fresh_lv1_state() {
        tokio::time::timeout(Duration::from_secs(2), async {
            let snapshot = || Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![scene_entry(1, "Intro")],
                channels: Vec::new(),
                ping_sequence: 0,
            };
            let event_bus = AppEventBus::default();
            let (show, show_task, _show_peers, mut lockout) =
                crate::show::build_show_actor(event_bus.clone());
            show_task.spawn();
            let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
            let runtime_generation = RuntimeGeneration::new();
            runtime_generation.set(1).await;
            let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
            let (scenes, scenes_task, peers) = build_scenes_actor(
                1,
                runtime_generation,
                event_bus.clone(),
                event_bus.subscribe(),
                fake_settings_handle(AppSettings::default()),
                AppSettings::default(),
                lockout.clone(),
            );
            peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1_tx.clone()), fade);
            scenes_task.spawn();
            install_scene_document(&scenes, intro_scene_document()).await;

            let set_lockout = |enabled| {
                let show = show.clone();
                async move {
                    let (reply, result) = oneshot::channel();
                    show.send(crate::show::ShowCommand::SetLockout {
                        enabled,
                        reply: Some(reply),
                    })
                    .await
                    .unwrap();
                    result.await.unwrap();
                }
            };

            let (reply, snapshot_wait) = oneshot::channel();
            scenes
                .send(ScenesCommand::RecallScene {
                    internal_scene_id: intro_internal_scene_id(),
                    reply,
                })
                .await
                .unwrap();
            let Some(Lv1Command::GetState { reply }) = lv1_rx.recv().await else {
                panic!("expected admission snapshot request");
            };
            set_lockout(true).await;
            assert!(lockout.changed().await.unwrap());
            assert!(reply.send(snapshot()).is_err());
            assert!(matches!(
                snapshot_wait.await.unwrap(),
                Err(AppCommandError::RecallCanceled(reason)) if reason == "lockout was enabled"
            ));

            set_lockout(false).await;
            assert!(!lockout.changed().await.unwrap());
            let (reply, capacity_wait) = oneshot::channel();
            scenes
                .send(ScenesCommand::RecallScene {
                    internal_scene_id: intro_internal_scene_id(),
                    reply,
                })
                .await
                .unwrap();
            for request in 0..2 {
                let Some(Lv1Command::GetState { reply }) = lv1_rx.recv().await else {
                    panic!("expected snapshot request {request}");
                };
                if request == 1 {
                    lv1_tx
                        .send(Lv1Command::WriteBatch(Vec::new()))
                        .await
                        .unwrap();
                }
                reply.send(snapshot()).unwrap();
            }
            yield_to_actor().await;
            set_lockout(true).await;
            assert!(lockout.changed().await.unwrap());
            assert!(matches!(
                lv1_rx.recv().await,
                Some(Lv1Command::WriteBatch(writes)) if writes.is_empty()
            ));
            assert!(matches!(
                capacity_wait.await.unwrap(),
                Err(AppCommandError::RecallCanceled(reason)) if reason == "lockout was enabled"
            ));
            assert!(lv1_rx.try_recv().is_err());

            scenes.send(ScenesCommand::Shutdown).await.unwrap();
        })
        .await
        .expect("recall checks should complete without sending a locked-out command");
    }

    #[tokio::test]
    async fn pending_store_snapshot_is_canceled_by_disconnect_before_lv1_replies() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let (show, show_task, _show_peers, lockout) =
            crate::show::build_show_actor(event_bus.clone());
        show_task.spawn();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            lockout,
        );
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1_tx), fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        mark_runtime_peers_ready_with_list(&handle, 1, vec![scene_entry(1, "Intro")]).await;

        let (reply, result) = oneshot::channel();
        handle
            .send(ScenesCommand::StoreSceneConfigFromCurrentLv1 {
                internal_scene_id: intro_internal_scene_id(),
                reply: Some(reply),
            })
            .await
            .unwrap();
        let Some(Lv1Command::GetState {
            reply: held_snapshot,
        }) = lv1_rx.recv().await
        else {
            panic!("expected held LV1 snapshot request");
        };

        event_bus.publish_lv1(
            1,
            Lv1Event::Disconnected {
                reason: "test disconnect".to_string(),
            },
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), result)
                .await
                .expect("disconnect should cancel before snapshot release")
                .unwrap(),
            Err("Store scene blocked: LV1 disconnected".to_string())
        );
        assert!(
            held_snapshot
                .send(Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: vec![scene_entry(1, "Intro")],
                    channels: Vec::new(),
                    ping_sequence: 0,
                })
                .is_err()
        );

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        show.send(crate::show::ShowCommand::SetLockout {
            enabled: false,
            reply: None,
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn abort_all_preempts_a_pending_store_snapshot() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        let (fade, mut fade_commands) = tokio::sync::mpsc::channel(1);
        let (show, show_task, _show_peers, lockout) =
            crate::show::build_show_actor(event_bus.clone());
        show_task.spawn();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            lockout,
        );
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1_tx), fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        mark_runtime_peers_ready_with_list(&handle, 1, vec![scene_entry(1, "Intro")]).await;

        let (store_reply, store_result) = oneshot::channel();
        handle
            .send(ScenesCommand::StoreSceneConfigFromCurrentLv1 {
                internal_scene_id: intro_internal_scene_id(),
                reply: Some(store_reply),
            })
            .await
            .unwrap();
        let Some(Lv1Command::GetState {
            reply: held_snapshot,
        }) = lv1_rx.recv().await
        else {
            panic!("expected held LV1 snapshot request");
        };

        let (abort_reply, abort_result) = oneshot::channel();
        handle
            .send(ScenesCommand::AbortAll { reply: abort_reply })
            .await
            .unwrap();

        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), store_result)
                .await
                .expect("Abort All should cancel the store before snapshot release")
                .unwrap(),
            Err("Store scene canceled: Abort All was requested".to_string())
        );
        let Some(FadeCommand::AbortAll {
            reply: Some(fade_reply),
        }) = fade_commands.recv().await
        else {
            panic!("expected Fade abort");
        };
        fade_reply.send(Ok(())).unwrap();
        assert_eq!(abort_result.await.unwrap(), Ok(()));
        assert!(
            held_snapshot
                .send(Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: vec![scene_entry(1, "Intro")],
                    channels: Vec::new(),
                    ping_sequence: 0,
                })
                .is_err()
        );

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        show.send(crate::show::ShowCommand::SetLockout {
            enabled: false,
            reply: None,
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn session_replacement_cancels_a_pending_store_snapshot() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        let (fade, _fade_commands) = tokio::sync::mpsc::channel(1);
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1_tx), fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        mark_runtime_peers_ready_with_list(&handle, 1, vec![scene_entry(1, "Intro")]).await;

        let (document_reply, document_result) = oneshot::channel();
        handle
            .send(ScenesCommand::GetSessionDocument {
                reply: document_reply,
            })
            .await
            .unwrap();
        let mut replacement = document_result.await.unwrap();
        replacement.scenes.scene_configs[0].duration_ms = 9_000;

        let (store_reply, store_result) = oneshot::channel();
        handle
            .send(ScenesCommand::StoreSceneConfigFromCurrentLv1 {
                internal_scene_id: intro_internal_scene_id(),
                reply: Some(store_reply),
            })
            .await
            .unwrap();
        let Some(Lv1Command::GetState {
            reply: held_snapshot,
        }) = lv1_rx.recv().await
        else {
            panic!("expected held LV1 snapshot request");
        };

        let (replace_reply, replace_result) = oneshot::channel();
        handle
            .send(ScenesCommand::ReplaceSessionDocument {
                replacement: crate::session::SessionReplacement::new(replacement.clone()),
                expected_generation: 1,
                reply: replace_reply,
            })
            .await
            .unwrap();
        assert_eq!(replace_result.await.unwrap().unwrap(), replacement);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), store_result)
                .await
                .expect("session replacement should cancel the held store")
                .unwrap(),
            Err("Store scene canceled: session was replaced".to_string())
        );
        assert!(
            held_snapshot
                .send(Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: vec![scene_entry(1, "Intro")],
                    channels: vec![crate::lv1::ChannelInfo {
                        group: 0,
                        channel: 1,
                        name: "Changed after replacement".to_string(),
                        gain_db: -20.0,
                        muted: false,
                        pan: None,
                        balance: None,
                        width: None,
                        pan_mode: None,
                    }],
                    ping_sequence: 10,
                })
                .is_err()
        );

        let (document_reply, document_result) = oneshot::channel();
        handle
            .send(ScenesCommand::GetSessionDocument {
                reply: document_reply,
            })
            .await
            .unwrap();
        assert_eq!(document_result.await.unwrap(), replacement);
        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test]
    async fn store_scene_config_from_current_lv1_preserves_empty_scope() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let scene_id = uuid::Uuid::from_u128(0x11111111111141118111111111111111);
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let _server = tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                if let crate::lv1::Lv1Command::GetState { reply } = command {
                    let _ = reply.send(Lv1StateSnapshot {
                        connection: ConnectionStatus::Connected,
                        scene: None,
                        scene_list: vec![SceneListEntry {
                            index: 3,
                            name: "Song 2 -- Changed".to_string(),
                        }],
                        channels: vec![crate::lv1::ChannelInfo {
                            group: 0,
                            channel: 2,
                            name: "Lead".to_string(),
                            gain_db: -8.0,
                            muted: false,
                            pan: None,
                            balance: None,
                            width: None,
                            pan_mode: None,
                        }],
                        ping_sequence: 0,
                    });
                }
            }
        });

        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();
        mark_runtime_peers_ready_with_list(&handle, 1, vec![scene_entry(3, "Song 2 -- Changed")])
            .await;

        crate::session::tests::replace_scenes(
            &handle,
            SceneDocument {
                scene_configs: vec![SceneConfig {
                    internal_scene_id: scene_id,
                    scene_index: Some(3),
                    scene_name: "Song 2 -- Changed".to_string(),
                    duration_ms: 1_000,
                    channel_configs: vec![],
                    scoped_channels: vec![],
                    scope_toggles: SceneScopeToggles {
                        faders: false,
                        pan: true,
                    },
                }],
                selected_scene_internal_id: None,
            },
            1,
        )
        .await;

        let mut events = event_bus.subscribe();

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::StoreSceneConfigFromCurrentLv1 {
                internal_scene_id: scene_id,
                reply: Some(reply),
            })
            .await
            .unwrap();

        assert_eq!(
            rx.await.unwrap().unwrap(),
            ScenesCommandResult { changed: true }
        );

        let event = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let AppEvent::Scenes {
                    generation: 1,
                    event,
                } = events.recv().await.unwrap()
                    && let ScenesEvent::StateChanged { .. } = event
                {
                    break event;
                }
            }
        })
        .await
        .expect("timed out waiting for scene state change");

        match event {
            ScenesEvent::StateChanged {
                persisted_scene_edit,
                state,
                ..
            } => {
                assert!(persisted_scene_edit);
                assert_eq!(state.scene_configs[0].scene_index, Some(3));
                assert_eq!(state.scene_configs[0].scene_name, "Song 2 -- Changed");
                assert!(state.scene_configs[0].scoped_channels.is_empty());
                assert!(!state.scene_configs[0].scope_toggles.faders);
                assert!(state.scene_configs[0].scope_toggles.pan);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn select_scene_config_does_not_publish_persisted_scene_edits() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        let scene_id = uuid::Uuid::from_u128(0x11111111111141118111111111111111);
        let (handle, task, _peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        task.spawn();

        crate::session::tests::replace_scenes(
            &handle,
            SceneDocument {
                scene_configs: vec![SceneConfig {
                    internal_scene_id: scene_id,
                    scene_index: Some(1),
                    scene_name: "Intro".to_string(),
                    duration_ms: 1_000,
                    channel_configs: vec![],
                    scoped_channels: vec![],
                    scope_toggles: Default::default(),
                }],
                selected_scene_internal_id: None,
            },
            0,
        )
        .await;

        let mut events = event_bus.subscribe();

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::SelectSceneConfig {
                internal_scene_id: scene_id,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap().unwrap().scene.internal_scene_id, scene_id);

        let select_event = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let AppEvent::Scenes {
                    generation: 1,
                    event,
                } = events.recv().await.unwrap()
                    && let ScenesEvent::StateChanged {
                        persisted_scene_edit,
                        ..
                    } = event
                {
                    break persisted_scene_edit;
                }
            }
        })
        .await
        .expect("timed out waiting for select scene state change");
        assert!(!select_event);
    }

    #[tokio::test(start_paused = true)]
    async fn copy_and_paste_scene_settings_publish_only_changed_projection_state() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (pending_scene_observed, pending_scene_ready) = oneshot::channel();
        let (handle, task, peers) = build_scenes_actor_with_pending_scene_observer(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
            pending_scene_observed,
        );
        let (lv1_tx, _lv1_rx) = tokio::sync::mpsc::channel(8);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1_tx), fade);
        task.spawn();
        mark_runtime_peers_ready_with_list(
            &handle,
            1,
            vec![
                scene_entry(1, "Intro"),
                scene_entry(2, "Verse"),
                scene_entry(3, "Chorus"),
            ],
        )
        .await;

        let source_id = uuid::Uuid::from_u128(1);
        let destination_id = uuid::Uuid::from_u128(2);
        let other_source_id = uuid::Uuid::from_u128(3);
        let mut source = intro_scene_document().scene_configs.remove(0);
        source.internal_scene_id = source_id;
        let mut destination = source.clone();
        destination.internal_scene_id = destination_id;
        destination.scene_index = Some(2);
        destination.scene_name = "Verse".to_string();
        destination.duration_ms = 1_000;
        destination.channel_configs.clear();
        destination.scoped_channels.clear();
        destination.scope_toggles = SceneScopeToggles::default();
        let mut other_source = source.clone();
        other_source.internal_scene_id = other_source_id;
        other_source.scene_index = Some(3);
        other_source.scene_name = "Chorus".to_string();
        other_source.duration_ms = 3_000;

        crate::session::tests::replace_scenes(
            &handle,
            SceneDocument {
                scene_configs: vec![source.clone(), destination, other_source.clone()],
                selected_scene_internal_id: None,
            },
            1,
        )
        .await;

        let mut events = event_bus.subscribe();

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::CopySceneSettings {
                source_internal_scene_id: source_id,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(
            rx.await.unwrap().unwrap(),
            ScenesCommandResult { changed: true }
        );
        let (persisted_scene_edit, state) =
            next_scene_state_change_for_generation(&mut events, 1).await;
        assert!(!persisted_scene_edit);
        assert!(state.scene_settings_clipboard_available);

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::CopySceneSettings {
                source_internal_scene_id: source_id,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(
            rx.await.unwrap().unwrap(),
            ScenesCommandResult { changed: false }
        );
        assert_no_scene_state_change_for_generation(&mut events, 1).await;

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::PasteSceneSettings {
                destination_internal_scene_id: destination_id,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(
            rx.await.unwrap().unwrap(),
            ScenesCommandResult { changed: true }
        );
        let (persisted_scene_edit, state) =
            next_scene_state_change_for_generation(&mut events, 1).await;
        assert!(persisted_scene_edit);
        assert!(state.scene_settings_clipboard_available);
        assert_eq!(state.scene_configs[1].duration_ms, source.duration_ms);

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::CopySceneSettings {
                source_internal_scene_id: other_source_id,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(
            rx.await.unwrap().unwrap(),
            ScenesCommandResult { changed: true }
        );
        assert_no_scene_state_change_for_generation(&mut events, 1).await;

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::PasteSceneSettings {
                destination_internal_scene_id: destination_id,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(
            rx.await.unwrap().unwrap(),
            ScenesCommandResult { changed: true }
        );
        let (persisted_scene_edit, state) =
            next_scene_state_change_for_generation(&mut events, 1).await;
        assert!(persisted_scene_edit);
        assert_eq!(state.scene_configs[1].duration_ms, other_source.duration_ms);

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::PasteSceneSettings {
                destination_internal_scene_id: destination_id,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(
            rx.await.unwrap().unwrap(),
            ScenesCommandResult { changed: false }
        );
        assert_no_scene_state_change_for_generation(&mut events, 1).await;

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::CopySceneSettings {
                source_internal_scene_id: uuid::Uuid::from_u128(4),
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), Err("Scene config not found".to_string()));
        assert_no_scene_state_change_for_generation(&mut events, 1).await;

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::PasteSceneSettings {
                destination_internal_scene_id: uuid::Uuid::from_u128(4),
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), Err("Scene config not found".to_string()));
        assert_no_scene_state_change_for_generation(&mut events, 1).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        pending_scene_ready
            .await
            .expect("scene observation should enter the pending state");

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::CopySceneSettings {
                source_internal_scene_id: other_source_id,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(
            rx.await.unwrap().unwrap(),
            ScenesCommandResult { changed: false }
        );
        assert_no_scene_state_change_for_generation(&mut events, 1).await;

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::PasteSceneSettings {
                destination_internal_scene_id: destination_id,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(
            rx.await.unwrap().unwrap(),
            ScenesCommandResult { changed: false }
        );
        assert_no_scene_state_change_for_generation(&mut events, 1).await;

        crate::session::tests::replace_scenes(&handle, SceneDocument::empty(), 1).await;
        let (persisted_scene_edit, state) =
            next_scene_state_change_for_generation(&mut events, 1).await;
        assert!(!persisted_scene_edit);
        assert!(!state.scene_settings_clipboard_available);

        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn stale_generation_does_not_start_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, _fade_rx, fade_starts) = fake_fade_handle();

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            lv1,
            fade,
            event_bus.clone(),
        )
        .await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;

        // Bump generation BEFORE the scene change — any fade started after this is stale
        runtime_generation.set(2).await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });

        // Advance past the 25 ms settle delay so the actor processes the scene change
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;

        // Assert no fade was started (generation guard should have blocked it)
        assert_eq!(
            fade_starts.load(Ordering::SeqCst),
            0,
            "expected zero fades but generation guard failed"
        );

        // Assert no StartRequested event was published
        let mut saw_start_requested = false;
        while let Ok(event) = events.try_recv() {
            if matches!(
                event,
                AppEvent::Scenes {
                    generation: 1,
                    event: crate::scenes::events::ScenesEvent::StartRequested { .. }
                }
            ) {
                saw_start_requested = true;
            }
        }
        assert!(
            !saw_start_requested,
            "StartRequested published despite stale generation"
        );

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        server.await.unwrap();
    }

    // The generation guard is checked before start_fade, but there is still a window between
    // the guard check and the actual start_fade call. This test pins that the guard fires
    // even when generation flips after the scene change event is published.
    #[tokio::test(start_paused = true)]
    async fn generation_flip_between_scene_change_and_fade_start_blocks_fade() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, _fade_rx, fade_starts) = fake_fade_handle();

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            lv1,
            fade,
            event_bus.clone(),
        )
        .await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;

        // Publish the scene change with generation still valid
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;

        // Flip generation while the actor is settling (before it dispatches start_fade)
        runtime_generation.set(2).await;

        // Now advance past the settle delay — policy will decide Start but generation is stale
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;

        assert_eq!(
            fade_starts.load(Ordering::SeqCst),
            0,
            "fade started despite generation flip before dispatch"
        );

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn valid_recall_starts_fade() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            lv1,
            fade,
            event_bus.clone(),
        )
        .await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;

        let (fade_command, behavior) = next_fade_command(&mut fade_rx).await;
        assert_eq!(behavior, SameSceneRecallBehavior::FinishActiveTargets);
        assert_eq!(
            fade_command.scene,
            FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string()
            }
        );
        assert_eq!(
            fade_command.targets,
            vec![FadeTarget {
                group: 0,
                channel: 2,
                parameter: FadeParameter::FaderDb,
                target: -12.5,
            }]
        );
        assert_eq!(fade_command.duration_ms, 4_000);
        assert!(matches!(fade_command.curve, FadeCurve::Linear));

        assert_eq!(fade_starts.load(Ordering::SeqCst), 1);

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn initial_settings_control_same_scene_behavior_and_repeat_delay() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, _fade_starts) = fake_fade_handle();
        let events = event_bus.subscribe();
        let settings = AppSettings {
            same_scene_recall_enabled: false,
            same_scene_recall_threshold_ms: 1_200,
            ..Default::default()
        };
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            events,
            fake_settings_handle(settings.clone()),
            settings,
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        let (_config, behavior) = next_fade_command(&mut fade_rx).await;
        assert_eq!(behavior, SameSceneRecallBehavior::OverrideMatchingTargets);

        tokio::time::advance(Duration::from_millis(900)).await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        assert!(matches!(
            fade_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));

        tokio::time::advance(Duration::from_millis(300)).await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        let (_config, behavior) = next_fade_command(&mut fade_rx).await;
        assert_eq!(behavior, SameSceneRecallBehavior::OverrideMatchingTargets);

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        drop(peers);
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn pending_settled_observation_settings_is_canceled_by_disconnect() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (settings, mut settings_commands) = tokio::sync::mpsc::channel(1);
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            settings,
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;

        let SettingsCommand::GetSettings {
            reply: held_settings,
        } = settings_commands.recv().await.unwrap()
        else {
            panic!("expected held settled-observation Settings request");
        };
        event_bus.publish_lv1(
            1,
            Lv1Event::Disconnected {
                reason: "test disconnect".to_string(),
            },
        );
        let (reply, state) = oneshot::channel();
        handle
            .send(ScenesCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), state)
                .await
                .expect("disconnect should release the gated scene mailbox")
                .unwrap()
                .ready_generation,
            None
        );
        assert!(held_settings.send(AppSettings::default()).is_err());
        handle.send(ScenesCommand::Shutdown).await.unwrap();
        drop(peers);
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn pending_settled_observation_snapshot_is_canceled_by_disconnect() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1_tx), fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        mark_runtime_peers_ready_with_list(&handle, 1, vec![scene_entry(1, "Intro")]).await;
        arm_recall_state(&event_bus).await;

        event_bus.publish_lv1(1, Lv1Event::SceneChanged(intro_scene()));
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        let Some(Lv1Command::GetState {
            reply: held_snapshot,
        }) = lv1_rx.recv().await
        else {
            panic!("expected held settled-observation snapshot request");
        };

        event_bus.publish_lv1(
            1,
            Lv1Event::Disconnected {
                reason: "test disconnect".to_string(),
            },
        );
        let (reply, state) = oneshot::channel();
        handle
            .send(ScenesCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), state)
                .await
                .expect("disconnect should release the gated scene mailbox")
                .unwrap()
                .ready_generation,
            None
        );
        assert!(
            held_snapshot
                .send(Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: Some(SceneState {
                        index: 1,
                        name: "Intro".to_string(),
                    }),
                    scene_list: vec![scene_entry(1, "Intro")],
                    channels: Vec::new(),
                    ping_sequence: 0,
                })
                .is_err()
        );
        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn pending_settled_observation_fade_capacity_is_canceled_by_disconnect() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx) = tokio::sync::mpsc::channel(1);
        fade.send(FadeCommand::AbortAll { reply: None })
            .await
            .unwrap();
        let (handoff_reached, handoff_wait) = oneshot::channel();
        let (resume_handoff, handoff_resume) = oneshot::channel();
        let (handle, task, peers) = build_scenes_actor_with_before_fade_handoff(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
            BeforeFadeHandoff {
                reached: handoff_reached,
                resume: handoff_resume,
            },
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;

        event_bus.publish_lv1(1, Lv1Event::SceneChanged(intro_scene()));
        tokio::time::advance(Duration::from_millis(50)).await;
        handoff_wait.await.unwrap();
        resume_handoff.send(()).unwrap();
        yield_to_actor().await;
        event_bus.publish_lv1(
            1,
            Lv1Event::Disconnected {
                reason: "test disconnect".to_string(),
            },
        );
        let (reply, state) = oneshot::channel();
        handle
            .send(ScenesCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(state.await.unwrap().ready_generation, None);
        assert!(matches!(
            fade_rx.recv().await,
            Some(FadeCommand::AbortAll { .. })
        ));
        yield_to_actor().await;
        assert!(fade_rx.try_recv().is_err());

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        drop(peers);
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn pending_settled_observation_fade_ack_is_canceled_by_disconnect() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx) = tokio::sync::mpsc::channel(1);
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;

        event_bus.publish_lv1(1, Lv1Event::SceneChanged(intro_scene()));
        tokio::time::advance(Duration::from_millis(50)).await;
        let held_ack = match fade_rx.recv().await {
            Some(FadeCommand::RecallSceneFade {
                reply: Some(reply), ..
            }) => reply,
            other => panic!("expected held Fade acknowledgement, got {other:?}"),
        };
        event_bus.publish_lv1(
            1,
            Lv1Event::Disconnected {
                reason: "test disconnect".to_string(),
            },
        );
        let (reply, state) = oneshot::channel();
        handle
            .send(ScenesCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(state.await.unwrap().ready_generation, None);
        assert!(held_ack.send(Ok(())).is_err());

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        drop(peers);
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn settled_observation_refreshes_current_settings_before_dispatch() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, _fade_starts) = fake_fade_handle();
        let disabled_settings = AppSettings {
            same_scene_recall_enabled: false,
            ..Default::default()
        };
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle_sequence(vec![AppSettings::default(), disabled_settings.clone()]),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        release_lv1.send(()).unwrap();
        arm_recall_state_for_generation(&event_bus, 1).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;

        let (_config, behavior) = next_fade_command(&mut fade_rx).await;
        assert_eq!(behavior, SameSceneRecallBehavior::OverrideMatchingTargets);

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        drop(peers);
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn settled_observation_stops_when_settings_refresh_fails() {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, _fade_starts) = fake_fade_handle();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle_then_unavailable(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;

        assert!(matches!(
            fade_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        assert!(captured
            .matching(
                "scene_recall_settings_unavailable",
                tracing::Level::ERROR,
            )
            .iter()
            .any(|log| {
                log.message.as_deref()
                    == Some(
                        "Scene recall automation stopped because current settings are unavailable",
                    )
        }));

        drop(peers);
        drop(handle);
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn live_settings_update_controls_same_scene_behavior_and_repeat_delay() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, _fade_starts) = fake_fade_handle();
        let disabled_settings = AppSettings {
            same_scene_recall_enabled: false,
            same_scene_recall_threshold_ms: 1_000,
            ..Default::default()
        };
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle_sequence(vec![
                AppSettings::default(),
                disabled_settings.clone(),
                disabled_settings.clone(),
                disabled_settings.clone(),
            ]),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        release_lv1.send(()).unwrap();
        arm_recall_state_for_generation(&event_bus, 1).await;

        event_bus.publish(AppEvent::Settings(SettingsEvent::StateChanged {
            settings: disabled_settings,
        }));
        yield_to_actor().await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        let (_config, behavior) = next_fade_command(&mut fade_rx).await;
        assert_eq!(behavior, SameSceneRecallBehavior::OverrideMatchingTargets);

        tokio::time::advance(Duration::from_millis(700)).await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        assert!(matches!(
            fade_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));

        tokio::time::advance(Duration::from_millis(300)).await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        let (_config, behavior) = next_fade_command(&mut fade_rx).await;
        assert_eq!(behavior, SameSceneRecallBehavior::OverrideMatchingTargets);

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        drop(peers);
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn settings_update_preserves_scene_settings_clipboard_and_pasted_recall_config() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, _fade_starts) = fake_fade_handle();
        let disabled_settings = AppSettings {
            same_scene_recall_enabled: false,
            same_scene_recall_threshold_ms: 1_000,
            ..Default::default()
        };
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(disabled_settings.clone()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();

        let source_id = uuid::Uuid::from_u128(0x22222222222242228222222222222222);
        let destination_id = intro_internal_scene_id();
        let mut destination_document = intro_scene_document();
        let destination = destination_document.scene_configs.remove(0);
        install_scene_document(
            &handle,
            SceneDocument {
                scene_configs: vec![
                    SceneConfig {
                        internal_scene_id: source_id,
                        scene_index: Some(2),
                        scene_name: "Source".to_string(),
                        duration_ms: 7_777,
                        channel_configs: vec![ChannelConfig {
                            group: 0,
                            channel: 2,
                            fader_db: Some(-3.0),
                            pan: None,
                            balance: None,
                            width: None,
                            pan_mode: None,
                        }],
                        scoped_channels: vec![ChannelRef {
                            group: 0,
                            channel: 2,
                        }],
                        scope_toggles: SceneScopeToggles {
                            faders: true,
                            pan: false,
                        },
                    },
                    destination,
                ],
                selected_scene_internal_id: None,
            },
        )
        .await;

        let (reply, copy_rx) = oneshot::channel();
        handle
            .send(ScenesCommand::CopySceneSettings {
                source_internal_scene_id: source_id,
                reply: Some(reply),
            })
            .await
            .unwrap();
        copy_rx.await.unwrap().unwrap();

        event_bus.publish(AppEvent::Settings(SettingsEvent::StateChanged {
            settings: disabled_settings,
        }));
        yield_to_actor().await;

        let (reply, paste_rx) = oneshot::channel();
        handle
            .send(ScenesCommand::PasteSceneSettings {
                destination_internal_scene_id: destination_id,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert!(paste_rx.await.unwrap().unwrap().changed);

        release_lv1.send(()).unwrap();
        arm_recall_state_for_generation(&event_bus, 1).await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        let (config, behavior) = next_fade_command(&mut fade_rx).await;
        assert_eq!(behavior, SameSceneRecallBehavior::OverrideMatchingTargets);
        assert_eq!(config.duration_ms, 7_777);
        assert_eq!(config.targets.len(), 1);
        assert_eq!(config.targets[0].target, -3.0);

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        drop(peers);
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn queued_settings_update_after_snapshot_supersedes_initial_policy() {
        let event_bus = AppEventBus::default();
        let events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let disabled_settings = AppSettings {
            same_scene_recall_enabled: false,
            same_scene_recall_threshold_ms: 1_000,
            ..Default::default()
        };
        event_bus.publish(AppEvent::Settings(SettingsEvent::StateChanged {
            settings: disabled_settings.clone(),
        }));
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, _fade_starts) = fake_fade_handle();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            events,
            fake_settings_handle(disabled_settings),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        release_lv1.send(()).unwrap();
        arm_recall_state_for_generation(&event_bus, 1).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        let (_config, behavior) = next_fade_command(&mut fade_rx).await;
        assert_eq!(behavior, SameSceneRecallBehavior::OverrideMatchingTargets);

        tokio::time::advance(Duration::from_millis(700)).await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        assert!(matches!(
            fade_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));

        tokio::time::advance(Duration::from_millis(300)).await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        let (_config, behavior) = next_fade_command(&mut fade_rx).await;
        assert_eq!(behavior, SameSceneRecallBehavior::OverrideMatchingTargets);

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        drop(peers);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn pending_lag_settings_refresh_is_canceled_by_generation_change() {
        let event_bus = AppEventBus::new(1);
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (settings, mut settings_commands) = tokio::sync::mpsc::channel(1);
        let (lv1, _lv1_commands) = tokio::sync::mpsc::channel(1);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation.clone(),
            event_bus.clone(),
            event_bus.subscribe(),
            settings,
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1), fade);
        task.spawn();
        mark_runtime_peers_ready_with_list(&handle, 1, vec![scene_entry(1, "Intro")]).await;

        for generation in 10..30 {
            event_bus.publish(AppEvent::Runtime(
                crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged {
                    generation,
                },
            ));
        }
        let SettingsCommand::GetSettings {
            reply: held_settings,
        } = tokio::time::timeout(Duration::from_secs(1), settings_commands.recv())
            .await
            .expect("lag recovery should request settings")
            .expect("settings mailbox should remain open")
        else {
            panic!("expected settings snapshot request");
        };

        runtime_generation.set(2).await;
        event_bus.publish(AppEvent::Runtime(
            crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged {
                generation: 2,
            },
        ));
        let (reply, state) = oneshot::channel();
        handle
            .send(ScenesCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), state)
                .await
                .expect("generation fact should remain responsive")
                .unwrap()
                .ready_generation,
            None
        );
        assert!(held_settings.send(AppSettings::default()).is_err());
        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test]
    async fn pending_lag_settings_failure_stops_actor() {
        let event_bus = AppEventBus::new(1);
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (settings, mut settings_commands) = tokio::sync::mpsc::channel(1);
        let (handle, task, _peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            settings,
            AppSettings::default(),
            test_lockout_reader(),
        );
        task.spawn();

        for generation in 10..30 {
            event_bus.publish(AppEvent::Runtime(
                crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged {
                    generation,
                },
            ));
        }
        let SettingsCommand::GetSettings { reply } =
            tokio::time::timeout(Duration::from_secs(1), settings_commands.recv())
                .await
                .expect("lag recovery should request settings")
                .expect("settings mailbox should remain open")
        else {
            panic!("expected settings snapshot request");
        };
        drop(reply);
        yield_to_actor().await;

        let (reply, _response) = oneshot::channel();
        assert!(
            handle
                .send(ScenesCommand::InitialProjectionState { reply })
                .await
                .is_err(),
            "failed lag recovery must stop the actor rather than later reuse stale policy"
        );
    }

    #[tokio::test]
    async fn pending_lag_lv1_snapshot_is_canceled_by_disconnect() {
        let event_bus = AppEventBus::new(1);
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let settings = fake_settings_handle(AppSettings::default());
        let (lv1, mut lv1_commands) = tokio::sync::mpsc::channel(1);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            settings,
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, crate::lv1::test_actor_handle(lv1), fade);
        task.spawn();
        mark_runtime_peers_ready_with_list(&handle, 1, vec![scene_entry(1, "Intro")]).await;

        for generation in 10..30 {
            event_bus.publish(AppEvent::Runtime(
                crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged {
                    generation,
                },
            ));
        }
        let Lv1Command::GetState {
            reply: held_snapshot,
        } = tokio::time::timeout(Duration::from_secs(1), lv1_commands.recv())
            .await
            .expect("lag recovery should request LV1 state")
            .expect("LV1 mailbox should remain open")
        else {
            panic!("expected LV1 snapshot request");
        };

        event_bus.publish_lv1(
            1,
            Lv1Event::Disconnected {
                reason: "test disconnect".to_string(),
            },
        );
        let (reply, state) = oneshot::channel();
        handle
            .send(ScenesCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), state)
                .await
                .expect("disconnect fact should remain responsive")
                .unwrap()
                .ready_generation,
            None
        );
        assert!(
            held_snapshot
                .send(Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: vec![scene_entry(1, "Intro")],
                    channels: Vec::new(),
                    ping_sequence: 0,
                })
                .is_err()
        );
        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn lagged_settings_events_refresh_before_recall() {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let event_bus = AppEventBus::new(1);
        let events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let settings_dir =
            std::env::temp_dir().join(format!("scene-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&settings_dir).unwrap();
        let (settings_handle, settings_task, _) =
            crate::settings::build_settings_actor(settings_dir, event_bus.clone());
        settings_task.spawn();
        let (reply, rx) = oneshot::channel();
        settings_handle
            .send(SettingsCommand::ReplaceSettings {
                settings: AppSettings {
                    same_scene_recall_enabled: false,
                    ..Default::default()
                },
                reply,
            })
            .await
            .unwrap();
        rx.await.unwrap().unwrap();
        runtime_generation.set(2).await;
        event_bus.publish(AppEvent::Runtime(
            crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged {
                generation: 1,
            },
        ));
        event_bus.publish(AppEvent::Runtime(
            crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged {
                generation: 2,
            },
        ));

        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let (handle, task, peers) = build_scenes_actor(
            2,
            runtime_generation,
            event_bus.clone(),
            events,
            settings_handle,
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(2, lv1, fade);
        task.spawn();
        release_lv1.send(()).unwrap();
        yield_to_actor().await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 2,
            event: Lv1Event::SceneListChanged(vec![scene_entry(1, "Intro")]),
        });
        yield_to_actor().await;
        install_scene_document_for_generation(&handle, intro_scene_document(), 2).await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 2,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        tokio::time::advance(Duration::from_millis(2_550)).await;
        yield_to_actor().await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 2,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        let (reply, response) = oneshot::channel();
        handle
            .send(ScenesCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(response.await.unwrap().ready_generation, Some(2));

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        drop(peers);
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn unavailable_settings_after_lag_stops_recall_automation() {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let event_bus = AppEventBus::new(1);
        let events = event_bus.subscribe();
        let (settings_tx, settings_rx) = tokio::sync::mpsc::channel(1);
        drop(settings_rx);
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, _fade_starts) = fake_fade_handle();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            events,
            settings_tx,
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(1, lv1, fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;

        event_bus.publish(AppEvent::Runtime(
            crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged {
                generation: 3,
            },
        ));
        event_bus.publish(AppEvent::Runtime(
            crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged {
                generation: 4,
            },
        ));
        yield_to_actor().await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;

        assert!(matches!(
            fade_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        assert!(captured
            .matching(
                "scene_recall_settings_unavailable",
                tracing::Level::ERROR,
            )
            .iter()
            .any(|log| {
                log.message.as_deref()
                    == Some(
                        "Scene recall automation stopped because current settings are unavailable",
                    )
        }));
        yield_to_actor().await;
        let (reply, _response) = oneshot::channel();
        assert!(
            handle
                .send(ScenesCommand::InitialProjectionState { reply })
                .await
                .is_err(),
            "failed lag recovery must stop the actor rather than later reuse stale policy"
        );
        drop(peers);
        drop(handle);
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn current_scene_move_sequence_does_not_start_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            lv1,
            fade,
            event_bus.clone(),
        )
        .await;
        release_lv1.send(()).unwrap();

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(song_3_at(4)),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(scene_list_before_current_move()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        yield_to_actor().await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(scene_list_after_current_move()),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(song_3_at(3)),
        });
        yield_to_actor().await;

        assert!(matches!(
            fade_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        assert_eq!(fade_starts.load(Ordering::SeqCst), 0);
        assert_no_scene_recall_event(&mut events).await;

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn non_current_rename_delayed_pair_does_not_start_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            lv1,
            fade,
            event_bus.clone(),
        )
        .await;
        release_lv1.send(()).unwrap();

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(song_3_at(4)),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(scene_list_before_non_current_rename()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        yield_to_actor().await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(scene_list_after_non_current_rename()),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(song_3_at(4)),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;

        assert!(matches!(
            fade_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        assert_eq!(fade_starts.load(Ordering::SeqCst), 0);
        assert_no_scene_recall_event(&mut events).await;

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn scene_changed_before_changed_scene_list_in_same_burst_does_not_start_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            lv1,
            fade,
            event_bus.clone(),
        )
        .await;
        release_lv1.send(()).unwrap();

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(song_3_at(4)),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(scene_list_before_current_move()),
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        tokio::task::yield_now().await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(song_3_at(3)),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(scene_list_after_current_move()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;

        assert!(matches!(
            fade_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        assert_eq!(fade_starts.load(Ordering::SeqCst), 0);
        assert_no_scene_recall_event(&mut events).await;

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn identical_scene_list_resend_does_not_block_real_recall() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            lv1,
            fade,
            event_bus.clone(),
        )
        .await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(500)).await;
        yield_to_actor().await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(vec![
                scene_entry(1, "Intro"),
                scene_entry(2, "Song 2"),
            ]),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(500)).await;
        yield_to_actor().await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(vec![
                scene_entry(1, "Intro"),
                scene_entry(2, "Song 2"),
            ]),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(vec![
                scene_entry(1, "Intro"),
                scene_entry(2, "Song 2"),
            ]),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;

        let (fade_command, behavior) = next_fade_command(&mut fade_rx).await;
        assert_eq!(behavior, SameSceneRecallBehavior::FinishActiveTargets);
        assert_eq!(
            fade_command.scene,
            FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string()
            }
        );
        assert_eq!(fade_starts.load(Ordering::SeqCst), 1);

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn valid_recall_after_scene_list_edit_window_starts_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            lv1,
            fade,
            event_bus.clone(),
        )
        .await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;
        yield_to_actor().await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(vec![
                scene_entry(1, "Intro"),
                scene_entry(2, "Song 2"),
            ]),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneListChanged(vec![
                scene_entry(1, "Intro"),
                scene_entry(2, "Song 2 -- Changed"),
            ]),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(500)).await;
        yield_to_actor().await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;

        let mut seen_ready = false;
        let mut seen_start_requested = false;
        for _ in 0..2 {
            match next_app_event_for_generation(&mut events, 1).await {
                AppEvent::Scenes {
                    generation: 1,
                    event: ScenesEvent::Ready { .. },
                } => seen_ready = true,
                AppEvent::Scenes {
                    generation: 1,
                    event: ScenesEvent::StartRequested { .. },
                } => seen_start_requested = true,
                other => panic!("unexpected event: {other:?}"),
            }
        }
        assert!(seen_ready && seen_start_requested);

        let (fade_command, behavior) = tokio::time::timeout(Duration::from_secs(1), fade_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(behavior, SameSceneRecallBehavior::FinishActiveTargets);
        assert_eq!(
            fade_command.scene,
            FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string()
            }
        );
        assert_eq!(fade_starts.load(Ordering::SeqCst), 1);
        assert_no_scene_recall_event(&mut events).await;

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn mismatched_fresh_lv1_snapshot_blocks_recall() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) =
            spawn_fake_lv1_with_mismatched_scene(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            lv1,
            fade,
            event_bus.clone(),
        )
        .await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;
        yield_to_actor().await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;
        tokio::time::advance(Duration::from_secs(2)).await;
        yield_to_actor().await;

        match next_scene_recall_event_for_generation(&mut events, 1).await {
            ScenesEvent::Blocked { reason, .. } => {
                assert!(
                    reason.contains("fresh LV1 scene did not match recalled scene")
                        || reason.contains("timed out waiting for fresh LV1 scene")
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(matches!(
            fade_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        assert_eq!(fade_starts.load(Ordering::SeqCst), 0);

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn empty_default_config_recall_skips_without_starting_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade_tx, mut fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = fade_tx;
        let handle = build_and_spawn_scene_recall_fader_with_document(
            1,
            runtime_generation.clone(),
            lv1,
            fade,
            event_bus.clone(),
            intro_scene_document_with_scope(SceneScopeToggles::default()),
        )
        .await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 1,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        match next_scene_recall_event_for_generation(&mut events, 1).await {
            ScenesEvent::Skipped {
                scene_label,
                reason,
            } => {
                assert_eq!(scene_label, "1: Intro");
                assert_eq!(reason, "no applicable targets");
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(matches!(
            fade_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        server.await.unwrap();
    }

    async fn assert_no_scene_recall_event(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
        tokio::task::yield_now().await;
        loop {
            match events.try_recv() {
                Ok(AppEvent::Scenes {
                    generation: 1,
                    event: ScenesEvent::StateChanged { .. },
                }) => continue,
                Ok(AppEvent::Scenes {
                    generation: 1,
                    event,
                }) => {
                    panic!("unexpected scene recall event: {event:?}")
                }
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => return,
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(count)) => {
                    panic!("unexpected lagged scene recall events: {count}")
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    panic!("event bus closed unexpectedly")
                }
            }
        }
    }

    async fn next_app_event_for_generation(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
        generation: u64,
    ) -> AppEvent {
        loop {
            let event = events.recv().await.unwrap();
            match event {
                AppEvent::Scenes {
                    generation: event_generation,
                    event: ScenesEvent::StateChanged { .. },
                } if event_generation == generation => continue,
                AppEvent::Scenes {
                    generation: event_generation,
                    ..
                } if event_generation == generation => return event,
                _ => continue,
            }
        }
    }

    async fn next_scene_recall_event_for_generation(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
        generation: u64,
    ) -> ScenesEvent {
        loop {
            if let AppEvent::Scenes {
                generation: event_generation,
                event,
            } = events.recv().await.unwrap()
                && event_generation == generation
                && !matches!(event, ScenesEvent::StateChanged { .. })
            {
                break event;
            }
        }
    }

    async fn next_scene_state_changed_for_generation(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
        generation: u64,
    ) -> crate::scenes::ScenesProjectionState {
        loop {
            if let AppEvent::Scenes {
                generation: event_generation,
                event: ScenesEvent::StateChanged { state, .. },
            } = events.recv().await.unwrap()
                && event_generation == generation
            {
                break state;
            }
        }
    }

    async fn next_scene_state_change_for_generation(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
        generation: u64,
    ) -> (bool, crate::scenes::ScenesProjectionState) {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::Scenes {
                    generation: event_generation,
                    event:
                        ScenesEvent::StateChanged {
                            state,
                            persisted_scene_edit,
                            ..
                        },
                } if event_generation == generation => break (persisted_scene_edit, state),
                AppEvent::SessionReplaced {
                    generation: event_generation,
                    scenes,
                    ..
                } if event_generation == generation => break (false, scenes),
                _ => {}
            }
        }
    }

    async fn assert_no_scene_state_change_for_generation(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
        generation: u64,
    ) {
        tokio::task::yield_now().await;
        while let Ok(event) = events.try_recv() {
            assert!(
                !matches!(
                    event,
                    AppEvent::Scenes {
                        generation: event_generation,
                        event: ScenesEvent::StateChanged { .. }
                    } if event_generation == generation
                ),
                "unexpected scene state change: {event:?}"
            );
        }
    }

    async fn next_scene_state_with_name_for_generation(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
        scene_name: &str,
        generation: u64,
    ) -> crate::scenes::ScenesProjectionState {
        loop {
            let state = next_scene_state_changed_for_generation(events, generation).await;
            if state
                .scene_configs
                .iter()
                .any(|scene| scene.scene_name == scene_name)
            {
                break state;
            }
        }
    }

    async fn next_blocked_scene_recall_event_for_generation(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
        generation: u64,
    ) -> bool {
        for _ in 0..3 {
            if matches!(
                next_scene_recall_event_for_generation(events, generation).await,
                ScenesEvent::Blocked { .. }
            ) {
                return true;
            }
        }
        false
    }

    type ObservedFadeCommand = (FadeConfig, SameSceneRecallBehavior);

    async fn next_fade_command(
        fade_rx: &mut tokio::sync::mpsc::Receiver<ObservedFadeCommand>,
    ) -> ObservedFadeCommand {
        for _ in 0..1_000 {
            match fade_rx.try_recv() {
                Ok(command) => return command,
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    yield_to_actor().await;
                    tokio::time::advance(Duration::from_millis(1)).await;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    panic!("fade command channel disconnected")
                }
            }
        }
        panic!("timed out waiting for fade command")
    }

    async fn spawn_fake_lv1_with_intro(
        _event_bus: AppEventBus,
    ) -> (
        Lv1ActorHandle,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let _ = release_rx.await;
            let snapshot = Lv1StateSnapshot {
                connection: crate::lv1::ConnectionStatus::Connected,
                scene: Some(intro_scene().scene),
                scene_list: vec![crate::lv1::SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                }],
                channels: vec![crate::lv1::ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Lead".to_string(),
                    gain_db: -8.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
                ping_sequence: 0,
            };
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    crate::lv1::Lv1Command::GetState { reply } => {
                        let _ = reply.send(snapshot.clone());
                    }
                    crate::lv1::Lv1Command::WriteBatch(_) => {}
                    crate::lv1::Lv1Command::SetGain { reply, .. } => {
                        let _ = reply.unwrap().send(Ok(()));
                    }
                    crate::lv1::Lv1Command::SetPan { reply, .. } => {
                        let _ = reply.unwrap().send(Ok(()));
                    }
                    crate::lv1::Lv1Command::SetBalance { reply, .. } => {
                        let _ = reply.unwrap().send(Ok(()));
                    }
                    crate::lv1::Lv1Command::SetWidth { reply, .. } => {
                        let _ = reply.unwrap().send(Ok(()));
                    }
                    crate::lv1::Lv1Command::SetMute { reply, .. } => {
                        let _ = reply.unwrap().send(Ok(()));
                    }
                    crate::lv1::Lv1Command::RecallScene { reply, .. } => {
                        let _ = reply.unwrap().send(Ok(RecallSceneDispatch {
                            scene_observation_sequence: 1,
                        }));
                    }
                    crate::lv1::Lv1Command::Flush { reply } => {
                        let _ = reply.unwrap().send(Ok(()));
                    }
                }
            }
        });
        (crate::lv1::test_actor_handle(lv1_tx), release_tx, server)
    }

    async fn spawn_fake_lv1_with_mismatched_scene(
        _event_bus: AppEventBus,
    ) -> (
        Lv1ActorHandle,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let _ = release_rx.await;
            let snapshot = Lv1StateSnapshot {
                connection: crate::lv1::ConnectionStatus::Connected,
                scene: Some(SceneState {
                    index: 2,
                    name: "Wrong".to_string(),
                }),
                scene_list: Vec::new(),
                channels: vec![crate::lv1::ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Lead".to_string(),
                    gain_db: -8.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
                ping_sequence: 0,
            };
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    crate::lv1::Lv1Command::GetState { reply } => {
                        let _ = reply.send(snapshot.clone());
                    }
                    crate::lv1::Lv1Command::WriteBatch(_) => {}
                    crate::lv1::Lv1Command::SetGain { reply, .. } => {
                        let _ = reply.unwrap().send(Ok(()));
                    }
                    crate::lv1::Lv1Command::SetPan { reply, .. } => {
                        let _ = reply.unwrap().send(Ok(()));
                    }
                    crate::lv1::Lv1Command::SetBalance { reply, .. } => {
                        let _ = reply.unwrap().send(Ok(()));
                    }
                    crate::lv1::Lv1Command::SetWidth { reply, .. } => {
                        let _ = reply.unwrap().send(Ok(()));
                    }
                    crate::lv1::Lv1Command::SetMute { reply, .. } => {
                        let _ = reply.unwrap().send(Ok(()));
                    }
                    crate::lv1::Lv1Command::RecallScene { reply, .. } => {
                        let _ = reply.unwrap().send(Ok(RecallSceneDispatch {
                            scene_observation_sequence: 1,
                        }));
                    }
                    crate::lv1::Lv1Command::Flush { reply } => {
                        let _ = reply.unwrap().send(Ok(()));
                    }
                }
            }
        });
        (crate::lv1::test_actor_handle(lv1_tx), release_tx, server)
    }

    fn intro_scene_document() -> SceneDocument {
        intro_scene_document_with_duration(4_000)
    }

    fn intro_scene_document_with_duration(duration_ms: u64) -> SceneDocument {
        SceneDocument {
            scene_configs: vec![SceneConfig {
                internal_scene_id: intro_internal_scene_id(),
                scene_index: Some(1),
                scene_name: "Intro".to_string(),
                duration_ms,
                channel_configs: vec![ChannelConfig {
                    group: 0,
                    channel: 2,
                    fader_db: Some(-12.5),
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
                scoped_channels: vec![ChannelRef {
                    group: 0,
                    channel: 2,
                }],
                scope_toggles: SceneScopeToggles {
                    faders: true,
                    pan: false,
                },
            }],
            selected_scene_internal_id: None,
        }
    }

    fn intro_scene_document_with_scope(scope_toggles: SceneScopeToggles) -> SceneDocument {
        let mut document = intro_scene_document_with_duration(0);
        document.scene_configs[0].channel_configs.clear();
        document.scene_configs[0].scoped_channels.clear();
        document.scene_configs[0].scope_toggles = scope_toggles;
        document
    }

    fn intro_internal_scene_id() -> uuid::Uuid {
        uuid::Uuid::from_u128(0x11111111111141118111111111111111)
    }

    fn fake_fade_handle() -> (
        FadeEngineHandle,
        tokio::sync::mpsc::Receiver<ObservedFadeCommand>,
        Arc<AtomicUsize>,
    ) {
        let (command_tx, mut command_rx) = tokio::sync::mpsc::channel(8);
        let (seen_tx, seen_rx) = tokio::sync::mpsc::channel(8);
        let starts = Arc::new(AtomicUsize::new(0));
        let starts_clone = starts.clone();
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                if let FadeCommand::RecallSceneFade {
                    config,
                    same_scene_behavior,
                    reply,
                    ..
                } = command
                {
                    let _ = seen_tx.send((config.clone(), same_scene_behavior)).await;
                    starts_clone.fetch_add(1, Ordering::SeqCst);
                    let _ = reply.unwrap().send(Ok(()));
                }
            }
        });
        (command_tx, seen_rx, starts)
    }

    fn fake_queue_fade_handle(
        event_bus: AppEventBus,
    ) -> (
        FadeEngineHandle,
        tokio::sync::mpsc::Receiver<QueueFadeCommand>,
    ) {
        let (command_tx, mut command_rx) = tokio::sync::mpsc::channel(8);
        let (seen_tx, seen_rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            let mut events = event_bus.subscribe();
            let mut readiness = None;
            loop {
                tokio::select! {
                    command = command_rx.recv() => match command {
                        Some(FadeCommand::RecallSceneFade { config, readiness: request, reply, .. }) => {
                            let _ = seen_tx.send(QueueFadeCommand::Recall { duration_ms: config.duration_ms }).await;
                            if let Some(reply) = reply {
                                let _ = reply.send(Ok(()));
                            }
                            readiness = request.completion.map(|completion| (1, 0, 0, completion));
                        }
                        Some(FadeCommand::WaitForRecallReadiness { scene: _, readiness: request, reply }) => {
                            let _ = seen_tx.send(QueueFadeCommand::Wait).await;
                            if let Some(reply) = reply {
                                let _ = reply.send(Ok(()));
                            }
                            readiness = request.completion.map(|completion| (1, 0, 0, completion));
                        }
                        Some(FadeCommand::AbortAll { reply }) => {
                            let _ = seen_tx.send(QueueFadeCommand::Abort).await;
                            if let Some(reply) = reply {
                                let _ = reply.send(Ok(()));
                            }
                        }
                        None => break,
                    },
                    event = events.recv() => match event {
                        Ok(AppEvent::Lv1 { generation, event: Lv1Event::PingReceived { sequence } }) => {
                            let Some((expected_generation, last_sequence, _, _)) = readiness.as_ref() else {
                                continue;
                            };
                            if generation != *expected_generation || sequence <= *last_sequence {
                                continue;
                            }
                            let released = {
                                let (_, last_sequence, count, _) = readiness.as_mut().expect("readiness must remain present");
                                *last_sequence = sequence;
                                *count += 1;
                                *count == 2
                            };
                            if !released {
                                continue;
                            }
                            let (_, _, _, completion) = readiness.take().expect("readiness must remain present");
                            let _ = completion.send(Ok(()));
                        }
                        Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    },
                }
            }
        });
        (command_tx, seen_rx)
    }

    fn intro_scene() -> SceneObservation {
        SceneObservation {
            sequence: 1,
            scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
        }
    }

    async fn build_and_spawn_scene_recall_fader(
        generation: u64,
        runtime_generation: RuntimeGeneration,
        lv1: Lv1ActorHandle,
        fade: FadeEngineHandle,
        event_bus: AppEventBus,
    ) -> ScenesHandle {
        build_and_spawn_scene_recall_fader_with_document(
            generation,
            runtime_generation,
            lv1,
            fade,
            event_bus,
            intro_scene_document(),
        )
        .await
    }

    async fn build_and_spawn_scene_recall_fader_with_document(
        generation: u64,
        runtime_generation: RuntimeGeneration,
        lv1: Lv1ActorHandle,
        fade: FadeEngineHandle,
        event_bus: AppEventBus,
        document: SceneDocument,
    ) -> ScenesHandle {
        let events = event_bus.subscribe();
        let (handle, task, peers) = build_scenes_actor(
            generation,
            runtime_generation,
            event_bus,
            events,
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers_for_generation(generation, lv1, fade);
        task.spawn();
        let initial_scene_list = document
            .scene_configs
            .iter()
            .filter_map(|scene| {
                scene.scene_index.map(|index| SceneListEntry {
                    index,
                    name: scene.scene_name.clone(),
                })
            })
            .collect();
        crate::session::tests::replace_scenes(&handle, document, generation).await;
        mark_runtime_peers_ready_with_list(&handle, generation, initial_scene_list).await;
        handle
    }

    async fn mark_runtime_peers_ready(handle: &ScenesHandle, generation: u64) {
        mark_runtime_peers_ready_with_list(handle, generation, Vec::new()).await;
    }

    async fn mark_runtime_peers_ready_with_list(
        handle: &ScenesHandle,
        generation: u64,
        initial_scene_list: Vec<SceneListEntry>,
    ) {
        let (reply, response) = oneshot::channel();
        handle
            .send(ScenesCommand::RuntimePeersReady {
                generation,
                initial_scene_list,
                reply,
            })
            .await
            .unwrap();
        response.await.unwrap().unwrap();
    }

    fn fake_settings_handle(settings: AppSettings) -> SettingsHandle {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if let SettingsCommand::GetSettings { reply } = command {
                    let _ = reply.send(settings.clone());
                }
            }
        });
        tx
    }

    fn fake_settings_handle_sequence(settings: Vec<AppSettings>) -> SettingsHandle {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            let mut settings = VecDeque::from(settings);
            while let Some(command) = rx.recv().await {
                if let SettingsCommand::GetSettings { reply } = command {
                    let _ = reply.send(settings.pop_front().expect("unexpected settings refresh"));
                }
            }
        });
        tx
    }

    fn fake_settings_handle_then_unavailable(settings: AppSettings) -> SettingsHandle {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            if let Some(SettingsCommand::GetSettings { reply }) = rx.recv().await {
                let _ = reply.send(settings);
            }
        });
        tx
    }

    async fn install_scene_document(handle: &ScenesHandle, document: SceneDocument) {
        install_scene_document_for_generation(handle, document, 1).await;
    }

    async fn install_scene_document_for_generation(
        handle: &ScenesHandle,
        document: SceneDocument,
        generation: u64,
    ) {
        let initial_scene_list = document
            .scene_configs
            .iter()
            .filter_map(|scene| {
                scene.scene_index.map(|index| SceneListEntry {
                    index,
                    name: scene.scene_name.clone(),
                })
            })
            .collect();
        crate::session::tests::replace_scenes(handle, document, generation).await;
        mark_runtime_peers_ready_with_list(handle, generation, initial_scene_list).await;
    }
}

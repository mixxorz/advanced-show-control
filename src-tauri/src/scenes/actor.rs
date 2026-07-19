use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::sync::{mpsc, oneshot};

use crate::fade::{
    FadeCommand, FadeEngineHandle, FadeSceneIdentity, RecallReadinessCancellation,
    RecallReadinessError, RecallReadinessRequest, SameSceneRecallBehavior,
};
use crate::lv1::{
    ConnectionStatus, Lv1ActorError, Lv1ActorHandle, Lv1Command, Lv1Event, Lv1StateSnapshot,
    SceneObservation, SceneState,
};
use crate::runtime::errors::AppCommandError;
use crate::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use crate::runtime::generation::RuntimeGeneration;
use crate::scenes::handle::ScenesHandle;
use crate::scenes::policy::{RecallPolicyDecision, RecallPolicyInput, decide_scene_recall};
use crate::scenes::recall_queue::{
    InFlightPhase, InFlightRecall, QueuedRecall, RECALL_COMPLETION_TIMEOUT, RECALL_QUEUE_CAPACITY,
    RecallQueue, RecallReadinessCompletion,
};
use crate::scenes::scene_alignment::scene_alignment_diagnostic;
use crate::scenes::{
    RecallSceneResult, SceneDocument, ScenesCommand, ScenesCommandResult, ScenesEvent,
    ScenesProjectionReason, ScenesState, SelectedSceneResult,
};
use crate::settings::{AppSettings, SettingsCommand, SettingsEvent, SettingsHandle};
use crate::show::ShowLockoutReader;

const SCENE_CHANGED_SETTLE_DELAY: std::time::Duration = std::time::Duration::from_millis(25);

#[derive(Clone, Default)]
pub struct ScenesPeers {
    peers: Arc<Mutex<Option<ScenesPeerHandles>>>,
}

#[derive(Clone)]
struct ScenesPeerHandles {
    lv1: Lv1ActorHandle,
    fade: FadeEngineHandle,
}

impl ScenesPeers {
    pub fn set_peers(&self, lv1: Lv1ActorHandle, fade: FadeEngineHandle) {
        *self.peers.lock().expect("scene recall peer lock poisoned") =
            Some(ScenesPeerHandles { lv1, fade });
    }

    fn handles(&self) -> ScenesPeerHandles {
        self.peers
            .lock()
            .expect("scene recall peer lock poisoned")
            .clone()
            .expect("scene recall peers must be set before use")
    }
}

struct PendingSceneObservation {
    generation: u64,
    sequence: u64,
    scene: SceneState,
    seen_at: tokio::time::Instant,
    settle_after: tokio::time::Instant,
}

impl PendingSceneObservation {
    fn new(generation: u64, sequence: u64, scene: SceneState, now: tokio::time::Instant) -> Self {
        Self {
            generation,
            sequence,
            scene,
            seen_at: now,
            settle_after: now + SCENE_CHANGED_SETTLE_DELAY,
        }
    }
}

pub struct ScenesTask {
    generation: u64,
    runtime_generation: RuntimeGeneration,
    peers: ScenesPeers,
    event_bus: AppEventBus,
    events: tokio::sync::broadcast::Receiver<AppEvent>,
    settings_handle: SettingsHandle,
    initial_settings: AppSettings,
    lockout: ShowLockoutReader,
    command_rx: mpsc::Receiver<ScenesCommand>,
    #[cfg(test)]
    pending_scene_observer: Option<oneshot::Sender<()>>,
    #[cfg(test)]
    before_fade_handoff: Option<BeforeFadeHandoff>,
}

#[cfg(test)]
struct BeforeFadeHandoff {
    reached: oneshot::Sender<()>,
    resume: oneshot::Receiver<()>,
}

impl ScenesTask {
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

    let handle = ScenesHandle::new(command_tx);
    let peers = ScenesPeers::default();
    let task = ScenesTask {
        generation,
        runtime_generation,
        peers: peers.clone(),
        events,
        event_bus,
        settings_handle,
        initial_settings,
        lockout,
        command_rx,
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

async fn run_scenes_actor(task: ScenesTask) {
    let ScenesTask {
        generation,
        runtime_generation,
        peers,
        event_bus,
        mut events,
        settings_handle,
        initial_settings,
        mut lockout,
        mut command_rx,
        #[cfg(test)]
        mut pending_scene_observer,
        #[cfg(test)]
        mut before_fade_handoff,
    } = task;

    let mut recall_state = ScenesState::default();
    let mut recall_queue = RecallQueue::default();
    let mut settings = initial_settings;
    let mut pending_scene: Option<PendingSceneObservation> = None;
    let mut lockout_open = true;
    let (readiness_completion_tx, mut readiness_completion_rx) = mpsc::channel(8);

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
        let recall_deadline = recall_queue
            .in_flight
            .as_ref()
            .and_then(|in_flight| match in_flight.phase {
                InFlightPhase::AwaitingObservation { deadline, .. } => Some(deadline),
                InFlightPhase::AwaitingReadiness { .. } => None,
            });
        let recall_timeout = async move {
            match recall_deadline {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => std::future::pending::<()>().await,
            }
        };
        if let Some(deadline) = pending_scene.as_ref().map(|pending| pending.settle_after) {
            tokio::select! {
                command = command_rx.recv() => {
                    let Some(command) = command else {
                        break;
                    };
                    if dispatch_scenes_command(
                        command,
                        &mut recall_state,
                        &mut recall_queue,
                        &peers,
                        &event_bus,
                        generation,
                        &lockout,
                    )
                    .await
                        == ScenesCommandDispatch::Shutdown
                    {
                        break;
                    }
                }
                event = events.recv() => {
                    match event {
                        Ok(AppEvent::Lv1 { generation: event_generation, event: Lv1Event::SceneListChanged(scene_list) }) => {
                            let before = recall_state.scene_configs().to_vec();
                            if recall_state.observe_and_align_scene_list(event_generation == generation, scene_list.clone(), tokio::time::Instant::now()) {
                                log_scene_alignment(&before, &recall_state, &scene_list);
                                publish_scene_state_changed(&event_bus, generation, ScenesProjectionReason::SceneState, &recall_state, true);
                            }
                        }
                        Ok(AppEvent::Lv1 { generation: event_generation, event: Lv1Event::SceneChanged(SceneObservation { sequence, scene }) }) => {
                            pending_scene = Some(PendingSceneObservation::new(event_generation, sequence, scene, tokio::time::Instant::now()));
                            #[cfg(test)]
                            if let Some(observer) = pending_scene_observer.take() {
                                let _ = observer.send(());
                            }
                        }
                        Ok(AppEvent::Settings(SettingsEvent::StateChanged { settings: updated_settings })) => {
                            settings = updated_settings;
                        }
                        Ok(AppEvent::Lv1 { generation: event_generation, event: Lv1Event::Disconnected { .. } }) if event_generation == generation => {
                            cancel_recall_queue(&mut recall_queue, "LV1 disconnected", true);
                        }
                        Ok(AppEvent::Runtime(crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged { generation: event_generation })) if event_generation != generation => {
                            cancel_recall_queue(&mut recall_queue, "LV1 connection generation changed", true);
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                            log_lagged_subscriber("scene-recall", count);
                            cancel_recall_queue(&mut recall_queue, "LV1 recall readiness was lost", true);
                            let Some(updated_settings) = refresh_settings_after_lag(&settings_handle).await else {
                                break;
                            };
                            settings = updated_settings;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            cancel_recall_queue(&mut recall_queue, "scene recall event stream closed", true);
                            break;
                        }
                    }
                }
                lockout_changed = lockout.changed(), if lockout_open => match lockout_changed {
                    Ok(true) => {
                        cancel_recall_queue(&mut recall_queue, "lockout was enabled", true);
                    }
                    Ok(false) => {}
                    Err(_) => {
                        cancel_recall_queue(&mut recall_queue, "lockout state is unavailable", true);
                        lockout_open = false;
                    }
                },
                _ = recall_timeout => {
                    cancel_recall_queue(&mut recall_queue, "LV1 recall readiness was lost", true);
                }
                _ = tokio::time::sleep_until(deadline) => {
                    if let Some(observation) = pending_scene.take() {
                        let Some(updated_settings) = refresh_settings_after_lag(&settings_handle).await else {
                            break;
                        };
                        if settings != updated_settings {
                            settings = updated_settings;
                        }
                        let peer_handles = peers.handles();
                        process_scene_observation(
                            generation,
                            &runtime_generation,
                            &peer_handles.lv1,
                            &peer_handles.fade,
                            &event_bus,
                            &mut recall_state,
                            &settings,
                            &lockout,
                            &mut recall_queue,
                            &readiness_completion_tx,
                            #[cfg(test)]
                            &mut before_fade_handoff,
                            observation,
                        ).await;
                    }
                }
                completion = readiness_completion_rx.recv() => {
                    let Some(completion) = completion else {
                        break;
                    };
                    handle_readiness_completion(
                        completion,
                        &runtime_generation,
                        &mut recall_queue,
                        &peers,
                        &lockout,
                        &recall_state,
                        generation,
                    ).await;
                }
            }
            continue;
        }

        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else {
                    break;
                };
                if dispatch_scenes_command(
                    command,
                    &mut recall_state,
                    &mut recall_queue,
                    &peers,
                    &event_bus,
                    generation,
                    &lockout,
                )
                .await
                    == ScenesCommandDispatch::Shutdown
                {
                    break;
                }
            }
            event = events.recv() => {
                match event {
                    Ok(AppEvent::Lv1 {
                        generation: event_generation,
                        event: Lv1Event::SceneListChanged(scene_list),
                    }) => {
                        let before = recall_state.scene_configs().to_vec();
                        if recall_state.observe_and_align_scene_list(event_generation == generation, scene_list.clone(), tokio::time::Instant::now()) {
                            log_scene_alignment(&before, &recall_state, &scene_list);
                            publish_scene_state_changed(&event_bus, generation, ScenesProjectionReason::SceneState, &recall_state, true);
                        }
                    }
                    Ok(AppEvent::Lv1 {
                        generation: event_generation,
                        event: Lv1Event::SceneChanged(SceneObservation { sequence, scene }),
                    }) => {
                        pending_scene = Some(PendingSceneObservation::new(
                            event_generation,
                            sequence,
                            scene,
                            tokio::time::Instant::now(),
                        ));
                        #[cfg(test)]
                        if let Some(observer) = pending_scene_observer.take() {
                            let _ = observer.send(());
                        }
                    }
                    Ok(AppEvent::Settings(SettingsEvent::StateChanged { settings: updated_settings })) => {
                        settings = updated_settings;
                    }
                    Ok(AppEvent::Lv1 { generation: event_generation, event: Lv1Event::Disconnected { .. } }) if event_generation == generation => {
                        cancel_recall_queue(&mut recall_queue, "LV1 disconnected", true);
                    }
                    Ok(AppEvent::Runtime(crate::runtime::events::RuntimeLifecycleEvent::ActiveGenerationChanged { generation: event_generation })) if event_generation != generation => {
                        cancel_recall_queue(&mut recall_queue, "LV1 connection generation changed", true);
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        log_lagged_subscriber("scene-recall", count);
                        cancel_recall_queue(&mut recall_queue, "LV1 recall readiness was lost", true);
                        let Some(updated_settings) = refresh_settings_after_lag(&settings_handle).await else {
                            break;
                        };
                        settings = updated_settings;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        cancel_recall_queue(&mut recall_queue, "scene recall event stream closed", true);
                        break;
                    }
                }
            }
            lockout_changed = lockout.changed(), if lockout_open => match lockout_changed {
                Ok(true) => {
                    cancel_recall_queue(&mut recall_queue, "lockout was enabled", true);
                }
                Ok(false) => {}
                Err(_) => {
                    cancel_recall_queue(&mut recall_queue, "lockout state is unavailable", true);
                    lockout_open = false;
                }
            },
            _ = recall_timeout => {
                cancel_recall_queue(&mut recall_queue, "LV1 recall readiness was lost", true);
            }
            completion = readiness_completion_rx.recv() => {
                let Some(completion) = completion else {
                    break;
                };
                handle_readiness_completion(
                    completion,
                    &runtime_generation,
                    &mut recall_queue,
                    &peers,
                    &lockout,
                    &recall_state,
                    generation,
                ).await;
            }
        }
    }

    cancel_recall_queue(&mut recall_queue, "Scenes actor stopped", true);
}

fn cancel_recall_queue(recall_queue: &mut RecallQueue, reason: &str, emit_log: bool) -> bool {
    let had_in_flight = recall_queue.in_flight.take().is_some();
    let had_waiting = !recall_queue.waiting.is_empty();
    recall_queue.drain_pending(AppCommandError::RecallCanceled(reason.to_string()));
    if emit_log && (had_in_flight || had_waiting) {
        tracing::warn!(
            event = "scene_recall_queue_cancelled",
            reason,
            "Queued scene recalls were canceled because {reason}"
        );
    }
    had_in_flight || had_waiting
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScenesCommandDispatch {
    Continue,
    Shutdown,
}

async fn dispatch_scenes_command(
    command: ScenesCommand,
    recall_state: &mut ScenesState,
    recall_queue: &mut RecallQueue,
    peers: &ScenesPeers,
    event_bus: &AppEventBus,
    generation: u64,
    lockout: &ShowLockoutReader,
) -> ScenesCommandDispatch {
    match command {
        ScenesCommand::GetSceneDocument { reply } => {
            let _ = reply.send(recall_state.snapshot());
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
        ScenesCommand::SetSceneDuration {
            internal_scene_id,
            duration_ms,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                ScenesProjectionReason::SceneState,
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
                ScenesProjectionReason::SceneState,
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
                ScenesProjectionReason::SceneState,
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
            let result = mutate_scene_state(
                recall_state,
                ScenesProjectionReason::SceneState,
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
            );
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
                ScenesProjectionReason::SceneState,
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
                ScenesProjectionReason::SceneState,
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
                ScenesProjectionReason::SceneState,
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
                        publish_scene_state_changed(
                            event_bus,
                            generation,
                            ScenesProjectionReason::SceneState,
                            recall_state,
                            true,
                        );
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
                ScenesProjectionReason::SceneState,
                true,
                |state| state.paste_scene_settings(destination_internal_scene_id),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::StoreSceneConfigFromCurrentLv1 {
            internal_scene_id,
            reply,
        } => {
            let peer_handles = peers.handles();
            let result = store_scene_config_from_current_lv1(
                &peer_handles.lv1,
                event_bus,
                generation,
                recall_state,
                internal_scene_id,
            )
            .await;
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::ReplaceSceneDocument {
            document,
            reason,
            persisted_scene_edit,
            reply,
        } => {
            recall_state.replace_snapshot_for_session(document);
            publish_scene_state_changed(
                event_bus,
                generation,
                reason,
                recall_state,
                persisted_scene_edit,
            );
            if let Some(reply) = reply {
                let _ = reply.send(ScenesCommandResult { changed: true });
            }
        }
        ScenesCommand::RecallScene {
            internal_scene_id,
            reply,
        } => {
            let peer_handles = peers.handles();
            admit_explicit_recall_scene(
                lockout,
                &peer_handles.lv1,
                recall_state,
                recall_queue,
                generation,
                internal_scene_id,
                reply,
            )
            .await;
        }
        ScenesCommand::AbortAll { reply } => {
            cancel_recall_queue(recall_queue, "Abort All was requested", true);
            let peer_handles = peers.handles();
            let (fade_reply, fade_result) = oneshot::channel();
            let result = match peer_handles
                .fade
                .send(FadeCommand::AbortAll {
                    reply: Some(fade_reply),
                })
                .await
            {
                Ok(()) => fade_result
                    .await
                    .map_err(|_| AppCommandError::ReplyChannelClosed)
                    .and_then(|result| result),
                Err(_) => Err(AppCommandError::FadeUnavailable),
            };
            let _ = reply.send(result);
        }
        ScenesCommand::Shutdown => return ScenesCommandDispatch::Shutdown,
    }
    ScenesCommandDispatch::Continue
}

async fn refresh_settings_after_lag(settings_handle: &SettingsHandle) -> Option<AppSettings> {
    let (reply, rx) = oneshot::channel();
    if settings_handle
        .send(SettingsCommand::GetSettings { reply })
        .await
        .is_err()
    {
        tracing::error!(
            event = "scene_recall_settings_unavailable",
            "Scene recall automation stopped because current settings are unavailable"
        );
        return None;
    }
    match rx.await {
        Ok(settings) => Some(settings),
        Err(_) => {
            tracing::error!(
                event = "scene_recall_settings_unavailable",
                "Scene recall automation stopped because current settings are unavailable"
            );
            None
        }
    }
}

async fn is_generation_current(expected: u64, runtime_generation: &RuntimeGeneration) -> bool {
    runtime_generation.current().await == expected
}

fn publish_scene_state_changed(
    event_bus: &AppEventBus,
    generation: u64,
    reason: ScenesProjectionReason,
    state: &ScenesState,
    persisted_scene_edit: bool,
) {
    event_bus.publish_scenes(
        generation,
        ScenesEvent::StateChanged {
            reason,
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
    reason: ScenesProjectionReason,
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
        publish_scene_state_changed(event_bus, generation, reason, state, persisted_scene_edit);
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
        publish_scene_state_changed(
            event_bus,
            generation,
            ScenesProjectionReason::SceneState,
            state,
            false,
        );
    }
    Ok(ScenesCommandResult {
        changed: result.contents_changed,
    })
}

async fn store_scene_config_from_current_lv1(
    lv1: &Lv1ActorHandle,
    event_bus: &AppEventBus,
    generation: u64,
    state: &mut ScenesState,
    internal_scene_id: uuid::Uuid,
) -> Result<ScenesCommandResult, String> {
    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::GetState { reply })
        .await
        .map_err(|_| "Store scene blocked: LV1 state is unavailable".to_string())?;
    let snapshot = rx
        .await
        .map_err(|_| "Store scene blocked: LV1 state is unavailable".to_string())?;
    let changed = state.store_scene_config(internal_scene_id, &snapshot.channels)?;
    if changed {
        publish_scene_state_changed(
            event_bus,
            generation,
            ScenesProjectionReason::SceneState,
            state,
            true,
        );
    }
    Ok(ScenesCommandResult { changed })
}

#[derive(Clone, Copy)]
struct QueueReadiness {
    request_id: uuid::Uuid,
    generation: u64,
    deadline: tokio::time::Instant,
}

impl QueueReadiness {
    fn attach_completion(
        self,
        completion_tx: mpsc::Sender<RecallReadinessCompletion>,
    ) -> RecallReadinessRequest {
        let (completion, completed) = oneshot::channel();
        let request_id = self.request_id;
        let generation = self.generation;
        tokio::spawn(async move {
            let result = completed
                .await
                .unwrap_or(Err(RecallReadinessError::Cancelled(
                    RecallReadinessCancellation::ActorStopped,
                )));
            let _ = completion_tx
                .send(RecallReadinessCompletion {
                    request_id,
                    generation,
                    result,
                })
                .await;
        });
        RecallReadinessRequest {
            deadline: self.deadline,
            completion: Some(completion),
        }
    }
}

fn exact_queue_readiness(
    observation: &PendingSceneObservation,
    recall_queue: &RecallQueue,
) -> Option<QueueReadiness> {
    let in_flight = recall_queue.in_flight.as_ref()?;
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

async fn handle_readiness_completion(
    completion: RecallReadinessCompletion,
    runtime_generation: &RuntimeGeneration,
    recall_queue: &mut RecallQueue,
    peers: &ScenesPeers,
    lockout: &ShowLockoutReader,
    recall_state: &ScenesState,
    generation: u64,
) {
    if runtime_generation.current().await != completion.generation {
        return;
    }
    let matches_in_flight = recall_queue.in_flight.as_ref().is_some_and(|in_flight| {
        in_flight.request_id == completion.request_id
            && in_flight.generation == completion.generation
            && matches!(in_flight.phase, InFlightPhase::AwaitingReadiness { .. })
    });
    if !matches_in_flight {
        return;
    }
    if let Err(RecallReadinessError::TimedOut {
        generation,
        scene_index,
        scene_name,
        observed_ping_count,
    }) = completion.result
    {
        if cancel_recall_queue(recall_queue, "LV1 recall readiness was lost", false) {
            tracing::warn!(
                event = "scene_recall_queue_cancelled",
                generation,
                scene_index,
                scene_name = %scene_name,
                observed_ping_count,
                timeout_ms = RECALL_COMPLETION_TIMEOUT.as_millis(),
                "Paused fades were aborted and queued scene recalls were canceled because LV1 did not resume its keepalive cadence after scene recall"
            );
        }
        return;
    }
    if completion.result.is_err() {
        cancel_recall_queue(recall_queue, "LV1 recall readiness was lost", true);
        return;
    }

    recall_queue.in_flight = None;
    let peer_handles = peers.handles();
    dispatch_next_recall(
        lockout,
        &peer_handles.lv1,
        recall_state,
        recall_queue,
        generation,
        Some(runtime_generation),
    )
    .await;
}

fn prepare_queue_readiness(
    recall_queue: &RecallQueue,
    readiness: QueueReadiness,
    readiness_completion_tx: mpsc::Sender<RecallReadinessCompletion>,
) -> Option<RecallReadinessRequest> {
    if tokio::time::Instant::now() >= readiness.deadline {
        return None;
    }
    let in_flight = recall_queue.in_flight.as_ref()?;
    if in_flight.request_id != readiness.request_id || in_flight.generation != readiness.generation
    {
        return None;
    }
    Some(readiness.attach_completion(readiness_completion_tx))
}

fn accept_queue_readiness(recall_queue: &mut RecallQueue, readiness: QueueReadiness) -> bool {
    let Some(in_flight) = recall_queue.in_flight.as_mut() else {
        return false;
    };
    if in_flight.request_id != readiness.request_id || in_flight.generation != readiness.generation
    {
        return false;
    }
    if !matches!(in_flight.phase, InFlightPhase::AwaitingObservation { .. }) {
        return false;
    }
    in_flight.phase = InFlightPhase::AwaitingReadiness {
        deadline: readiness.deadline,
    };
    true
}

#[allow(clippy::too_many_arguments)]
async fn process_scene_observation(
    generation: u64,
    runtime_generation: &RuntimeGeneration,
    lv1: &Lv1ActorHandle,
    fade: &FadeEngineHandle,
    event_bus: &AppEventBus,
    recall_state: &mut ScenesState,
    settings: &AppSettings,
    lockout: &ShowLockoutReader,
    recall_queue: &mut RecallQueue,
    readiness_completion_tx: &mpsc::Sender<RecallReadinessCompletion>,
    #[cfg(test)] before_fade_handoff: &mut Option<BeforeFadeHandoff>,
    observation: PendingSceneObservation,
) {
    let now = tokio::time::Instant::now();
    let queue_readiness = exact_queue_readiness(&observation, recall_queue);
    if queue_readiness
        .as_ref()
        .is_some_and(|readiness| now >= readiness.deadline)
    {
        cancel_recall_queue(recall_queue, "LV1 recall readiness was lost", true);
        return;
    }

    let skipped_reason = if recall_state.is_scene_list_edit_suppressed(observation.seen_at)
        || recall_state.is_scene_list_edit_suppressed(now)
    {
        Some("scene list edit suppression".to_string())
    } else {
        let same_scene_repeat_delay =
            std::time::Duration::from_millis(settings.same_scene_recall_threshold_ms);
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

    if !is_generation_current(generation, runtime_generation).await {
        return;
    }

    let lv1_snapshot = match fresh_lv1_snapshot(lv1, &observation.scene).await {
        Ok(snapshot) => snapshot,
        Err(err) => {
            if !is_generation_current(generation, runtime_generation).await {
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
    let initial_lockout = lockout.current();

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
            lockout: initial_lockout,
            scene_config: scene_config.clone(),
        })
    };
    match decision {
        RecallPolicyDecision::Start(fade_config) => {
            let scene_label = scene_label(&observation.scene);
            let queued_handoff = queue_readiness.is_some();
            #[cfg(test)]
            if let Some(BeforeFadeHandoff { reached, resume }) = before_fade_handoff.take() {
                let _ = reached.send(());
                let _ = resume.await;
            }
            let (fade_config, readiness, queued_readiness) =
                if let Some(queue_readiness) = queue_readiness {
                    // Lockout can change while the settled observation is awaiting
                    // handoff, so policy is re-evaluated at the final safe point.
                    let current_lockout = lockout.current();
                    if current_lockout {
                        cancel_recall_queue(recall_queue, "lockout was enabled", true);
                        return;
                    }
                    let RecallPolicyDecision::Start(fade_config) =
                        decide_scene_recall(RecallPolicyInput {
                            recalled_scene: observation.scene.clone(),
                            lv1_snapshot,
                            lockout: current_lockout,
                            scene_config,
                        })
                    else {
                        cancel_recall_queue(recall_queue, "LV1 recall readiness was lost", true);
                        return;
                    };
                    let Some(readiness) = prepare_queue_readiness(
                        recall_queue,
                        queue_readiness,
                        readiness_completion_tx.clone(),
                    ) else {
                        cancel_recall_queue(recall_queue, "LV1 recall readiness was lost", true);
                        return;
                    };
                    (fade_config, readiness, Some(queue_readiness))
                } else {
                    (
                        fade_config,
                        RecallReadinessRequest::detached(
                            tokio::time::Instant::now() + Duration::from_secs(5),
                        ),
                        None,
                    )
                };
            if !is_generation_current(generation, runtime_generation).await {
                return;
            }
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
            let (reply, rx) = oneshot::channel();
            let same_scene_behavior = if settings.same_scene_recall_enabled {
                SameSceneRecallBehavior::FinishActiveTargets
            } else {
                SameSceneRecallBehavior::OverrideMatchingTargets
            };
            let permit = match fade.reserve().await {
                Ok(permit) => permit,
                Err(_) => {
                    if queued_handoff {
                        cancel_recall_queue(recall_queue, "Fade engine is unavailable", true);
                    }
                    event_bus.publish_scenes(
                        generation,
                        ScenesEvent::Blocked {
                            scene_label,
                            reason: "failed to start fade: FadeUnavailable".to_string(),
                        },
                    );
                    return;
                }
            };
            let result = if !is_generation_current(generation, runtime_generation).await {
                Err(AppCommandError::StaleGeneration)
            } else if queued_handoff && lockout.current() {
                cancel_recall_queue(recall_queue, "lockout was enabled", true);
                return;
            } else {
                permit.send(FadeCommand::RecallSceneFade {
                    config: fade_config,
                    same_scene_behavior,
                    expected_generation: Some(generation),
                    readiness,
                    reply: Some(reply),
                });
                match rx.await {
                    Ok(result) => result,
                    Err(_) => Err(AppCommandError::ReplyChannelClosed),
                }
            };
            match result {
                Ok(()) => {
                    if let Some(queued_readiness) = queued_readiness
                        && !accept_queue_readiness(recall_queue, queued_readiness)
                    {
                        cancel_recall_queue(recall_queue, "LV1 recall readiness was lost", true);
                    }
                }
                Err(AppCommandError::StaleGeneration) if queued_handoff => {
                    cancel_recall_queue(recall_queue, "LV1 connection generation changed", true);
                }
                Err(AppCommandError::StaleGeneration) => (),
                Err(err) => {
                    if queued_handoff {
                        cancel_recall_queue(recall_queue, "Fade engine is unavailable", true);
                    }
                    event_bus.publish_scenes(
                        generation,
                        ScenesEvent::Blocked {
                            scene_label,
                            reason: format!("failed to start fade: {err:?}"),
                        },
                    );
                }
            }
        }
        RecallPolicyDecision::Skip { reason } => {
            if !is_generation_current(generation, runtime_generation).await {
                return;
            }
            event_bus.publish_scenes(
                generation,
                ScenesEvent::Skipped {
                    scene_label: scene_label(&observation.scene),
                    reason,
                },
            );
            if let Some(queue_readiness) = queue_readiness {
                if lockout.current() {
                    cancel_recall_queue(recall_queue, "lockout was enabled", true);
                    return;
                }
                let Some(readiness) = prepare_queue_readiness(
                    recall_queue,
                    queue_readiness,
                    readiness_completion_tx.clone(),
                ) else {
                    cancel_recall_queue(recall_queue, "LV1 recall readiness was lost", true);
                    return;
                };
                let Ok(permit) = fade.reserve().await else {
                    cancel_recall_queue(recall_queue, "Fade engine is unavailable", true);
                    return;
                };
                if !is_generation_current(generation, runtime_generation).await {
                    cancel_recall_queue(recall_queue, "LV1 connection generation changed", true);
                    return;
                }
                if lockout.current() {
                    cancel_recall_queue(recall_queue, "lockout was enabled", true);
                    return;
                }
                let (reply, rx) = oneshot::channel();
                permit.send(FadeCommand::WaitForRecallReadiness {
                    scene: FadeSceneIdentity {
                        index: observation.scene.index,
                        name: observation.scene.name.clone(),
                    },
                    expected_generation: generation,
                    readiness,
                    reply: Some(reply),
                });
                match rx.await {
                    Ok(Ok(())) if accept_queue_readiness(recall_queue, queue_readiness) => {}
                    Ok(Ok(())) => {
                        cancel_recall_queue(recall_queue, "LV1 recall readiness was lost", true);
                    }
                    Ok(Err(_)) | Err(_) => {
                        cancel_recall_queue(recall_queue, "Fade engine is unavailable", true);
                    }
                }
            }
        }
        RecallPolicyDecision::Blocked { reason } => {
            if !is_generation_current(generation, runtime_generation).await {
                return;
            }
            let scene_label = scene_label(&observation.scene);
            tracing::warn!(
                event = "scene_recall_blocked",
                scene = %scene_label,
                reason = %reason,
                "Scene recall blocked for {scene_label}: {reason}"
            );
            event_bus.publish_scenes(
                generation,
                ScenesEvent::Blocked {
                    scene_label,
                    reason,
                },
            );
            if let Some(queue_readiness) = queue_readiness {
                if lockout.current() {
                    cancel_recall_queue(recall_queue, "lockout was enabled", true);
                    return;
                }
                let Some(readiness) = prepare_queue_readiness(
                    recall_queue,
                    queue_readiness,
                    readiness_completion_tx.clone(),
                ) else {
                    cancel_recall_queue(recall_queue, "LV1 recall readiness was lost", true);
                    return;
                };
                let Ok(permit) = fade.reserve().await else {
                    cancel_recall_queue(recall_queue, "Fade engine is unavailable", true);
                    return;
                };
                if !is_generation_current(generation, runtime_generation).await {
                    cancel_recall_queue(recall_queue, "LV1 connection generation changed", true);
                    return;
                }
                if lockout.current() {
                    cancel_recall_queue(recall_queue, "lockout was enabled", true);
                    return;
                }
                let (reply, rx) = oneshot::channel();
                permit.send(FadeCommand::WaitForRecallReadiness {
                    scene: FadeSceneIdentity {
                        index: observation.scene.index,
                        name: observation.scene.name.clone(),
                    },
                    expected_generation: generation,
                    readiness,
                    reply: Some(reply),
                });
                match rx.await {
                    Ok(Ok(())) if accept_queue_readiness(recall_queue, queue_readiness) => {}
                    Ok(Ok(())) => {
                        cancel_recall_queue(recall_queue, "LV1 recall readiness was lost", true);
                    }
                    Ok(Err(_)) | Err(_) => {
                        cancel_recall_queue(recall_queue, "Fade engine is unavailable", true);
                    }
                }
            }
        }
    }
}

fn scene_label(scene: &SceneState) -> String {
    format!("{}: {}", scene.index, scene.name)
}

async fn admit_explicit_recall_scene(
    lockout: &ShowLockoutReader,
    lv1: &Lv1ActorHandle,
    recall_state: &ScenesState,
    recall_queue: &mut RecallQueue,
    generation: u64,
    internal_scene_id: uuid::Uuid,
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
            log_explicit_recall_blocked(internal_scene_id, &error);
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
    if recall_queue.is_full() {
        tracing::warn!(
            event = "scene_recall_queue_full",
            internal_scene_id = %internal_scene_id,
            capacity = RECALL_QUEUE_CAPACITY,
            "Scene recall blocked because the recall queue is full"
        );
        let _ = reply.send(Err(AppCommandError::RecallQueueFull));
        return;
    }

    recall_queue.admit(QueuedRecall {
        request_id: uuid::Uuid::new_v4(),
        internal_scene_id,
        reply,
    });
    if recall_queue.in_flight.is_none() {
        dispatch_next_recall(lockout, lv1, recall_state, recall_queue, generation, None).await;
    }
}

async fn dispatch_next_recall(
    lockout: &ShowLockoutReader,
    lv1: &Lv1ActorHandle,
    recall_state: &ScenesState,
    recall_queue: &mut RecallQueue,
    generation: u64,
    runtime_generation: Option<&RuntimeGeneration>,
) {
    while let Some(queued) = recall_queue.take_next() {
        let lv1_snapshot = match explicit_recall_lv1_snapshot(lv1).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                log_explicit_recall_blocked(queued.internal_scene_id, &error);
                let _ = queued.reply.send(Err(error));
                cancel_recall_queue(recall_queue, "LV1 state is unavailable", true);
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

        let permit = match lv1.reserve().await {
            Ok(permit) => permit,
            Err(error) => {
                let error = AppCommandError::CommandFailed(error.to_string());
                log_explicit_recall_blocked(queued.internal_scene_id, &error);
                let _ = queued.reply.send(Err(error));
                cancel_recall_queue(recall_queue, "LV1 state is unavailable", true);
                return;
            }
        };
        if let Some(runtime_generation) = runtime_generation
            && runtime_generation.current().await != generation
        {
            cancel_recall_queue(recall_queue, "LV1 connection generation changed", true);
            return;
        }
        if lockout.current() {
            cancel_recall_queue(recall_queue, "lockout was enabled", true);
            return;
        }
        let (reply, rx) = oneshot::channel();
        permit.send(Lv1Command::RecallScene {
            scene_index: result.lv1_scene_index,
            reply: Some(reply),
        });
        let dispatch = rx
            .await
            .map_err(|_| AppCommandError::ReplyChannelClosed)
            .and_then(|result| {
                result.map_err(|error| match error {
                    Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
                    other => AppCommandError::CommandFailed(other.to_string()),
                })
            });
        let dispatch = match dispatch {
            Ok(dispatch) => dispatch,
            Err(error) => {
                log_explicit_recall_blocked(queued.internal_scene_id, &error);
                let _ = queued.reply.send(Err(error));
                cancel_recall_queue(recall_queue, "LV1 recall command is unavailable", true);
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
        recall_queue.set_in_flight(InFlightRecall {
            request_id: queued.request_id,
            generation,
            result: result.clone(),
            phase: InFlightPhase::AwaitingObservation {
                dispatch_sequence: dispatch.scene_observation_sequence,
                deadline: tokio::time::Instant::now() + RECALL_COMPLETION_TIMEOUT,
            },
        });
        let _ = queued.reply.send(Ok(result));
        return;
    }
}

async fn explicit_recall_lv1_snapshot(
    lv1: &Lv1ActorHandle,
) -> Result<Lv1StateSnapshot, AppCommandError> {
    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::GetState { reply })
        .await
        .map_err(|error| match error {
            Lv1ActorError::NotConnected => AppCommandError::CommandFailed(
                "Recall blocked: LV1 state is unavailable".to_string(),
            ),
            other => AppCommandError::CommandFailed(other.to_string()),
        })?;
    rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)
}

fn validate_explicit_recall(
    lockout: &ShowLockoutReader,
    scene_document: &SceneDocument,
    lv1_snapshot: &Lv1StateSnapshot,
    internal_scene_id: uuid::Uuid,
) -> Result<RecallSceneResult, AppCommandError> {
    crate::scenes::validate_recall_scene_request(
        lockout.current(),
        scene_document,
        lv1_snapshot,
        internal_scene_id,
    )
    .map_err(AppCommandError::CommandFailed)
}

fn log_explicit_recall_blocked(internal_scene_id: uuid::Uuid, error: &AppCommandError) {
    tracing::warn!(
        event = "scene_recall_blocked",
        internal_scene_id = %internal_scene_id,
        reason = %error,
        "Scene recall blocked: {error}"
    );
}

async fn fresh_lv1_snapshot(
    lv1: &Lv1ActorHandle,
    scene: &SceneState,
) -> Result<Lv1StateSnapshot, AppCommandError> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let (reply, rx) = oneshot::channel();
        lv1.send(Lv1Command::GetState { reply })
            .await
            .map_err(|error| match error {
                crate::lv1::Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
                other => AppCommandError::CommandFailed(other.to_string()),
            })?;
        let snapshot = rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?;
        if snapshot.connection == ConnectionStatus::Connected
            && snapshot.scene.as_ref() == Some(scene)
        {
            return Ok(snapshot);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(AppCommandError::CommandFailed(format!(
                "timed out waiting for fresh LV1 scene to match recalled scene {}: {}",
                scene.index, scene.name
            )));
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::{
        FadeCommand, FadeConfig, FadeCurve, FadeEngineHandle, FadeParameter, FadeSceneIdentity,
        FadeTarget,
    };
    use crate::lv1::{
        Lv1ActorHandle, Lv1Event, Lv1StateSnapshot, RecallSceneDispatch, SceneListEntry,
        SceneObservation, SceneState,
    };
    use crate::scenes::events::ScenesEvent;
    use crate::scenes::{ChannelConfig, ChannelRef, SceneConfig, SceneDocument, SceneScopeToggles};
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
        let (_show, _task, _peers, lockout) = crate::show::build_show_actor(AppEventBus::default());
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

    struct RecallQueueFixture {
        handle: ScenesHandle,
        show: crate::show::ShowStateHandle,
        event_bus: AppEventBus,
        runtime_generation: RuntimeGeneration,
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
            let fade = FadeEngineHandle::new(fade_tx);
            let (handle, task, peers) = build_scenes_actor(
                1,
                runtime_generation,
                event_bus.clone(),
                event_bus.subscribe(),
                fake_settings_handle(AppSettings::default()),
                AppSettings::default(),
                lockout.clone(),
            );
            peers.set_peers(lv1, fade.clone());
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

    impl RecallQueueFixture {
        async fn connected_with_scenes(scene_configs: Vec<SceneConfig>) -> Self {
            Self::connected_with_scenes_and_handoff_gate(scene_configs, None).await
        }

        async fn connected_with_scenes_and_handoff_gate(
            scene_configs: Vec<SceneConfig>,
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
                        Lv1Command::RecallScene { reply: None, .. } => {
                            panic!("queued recalls require an LV1 reply");
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
                    runtime_generation.clone(),
                    event_bus.clone(),
                    event_bus.subscribe(),
                    fake_settings_handle(AppSettings::default()),
                    AppSettings::default(),
                    lockout,
                    before_fade_handoff,
                ),
                None => build_scenes_actor(
                    1,
                    runtime_generation.clone(),
                    event_bus.clone(),
                    event_bus.subscribe(),
                    fake_settings_handle(AppSettings::default()),
                    AppSettings::default(),
                    lockout,
                ),
            };
            peers.set_peers(lv1, fade);
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

    async fn arm_queue_recall_gate(fixture: &RecallQueueFixture) {
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

    async fn enqueue_in_flight_and_waiting(
        fixture: &mut RecallQueueFixture,
    ) -> oneshot::Receiver<Result<RecallSceneResult, AppCommandError>> {
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        fixture.send_recall(uuid::Uuid::from_u128(2)).await
    }

    #[tokio::test(start_paused = true)]
    async fn recall_queue_cancellation_pre_observation_timeout_cancels_waiting_without_aborting_fade()
     {
        let captured = TracingCapture::new();
        let _guard = captured.install();
        let mut fixture = RecallQueueFixture::connected_with_scenes(vec![
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

        tokio::time::advance(RECALL_COMPLETION_TIMEOUT + Duration::from_millis(1)).await;
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

    #[tokio::test]
    async fn recall_queue_cancellation_matching_disconnect_cancels_waiting_but_stale_disconnect_does_not()
     {
        let mut fixture = RecallQueueFixture::connected_with_scenes(vec![
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
    }

    #[tokio::test]
    async fn recall_queue_cancellation_active_generation_change_cancels_waiting_but_stale_fact_does_not()
     {
        let mut fixture = RecallQueueFixture::connected_with_scenes(vec![
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
        let mut fixture = RecallQueueFixture::connected_with_scenes(vec![
            queue_scene(1, "Intro"),
            queue_scene(2, "Verse"),
        ])
        .await;
        let waiting = enqueue_in_flight_and_waiting(&mut fixture).await;
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

    #[tokio::test]
    async fn recall_queue_cancellation_abort_all_cancels_intent_before_forwarding_one_fade_abort() {
        let mut fixture = RecallQueueFixture::connected_with_scenes(vec![
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

    #[tokio::test]
    async fn recall_queue_first_recall_dispatches_and_second_reply_stays_pending() {
        let mut fixture = RecallQueueFixture::connected_with_scenes(vec![
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
        let mut fixture = RecallQueueFixture::connected_with_scenes(vec![
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
        tokio::time::sleep(Duration::from_millis(30)).await;

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
    async fn recall_queue_rechecks_lockout_immediately_before_fade_handoff() {
        let (reached, reached_rx) = oneshot::channel();
        let (resume, resume_rx) = oneshot::channel();
        let mut fixture = RecallQueueFixture::connected_with_scenes_and_handoff_gate(
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
        resume.send(()).unwrap();

        yield_to_actor().await;
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
        let mut fixture = RecallQueueFixture::connected_with_scenes(vec![
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
        peers.set_peers(lv1, fade);
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
    async fn recall_queue_keeps_repeated_same_scene_requests_distinct() {
        let mut fixture =
            RecallQueueFixture::connected_with_scenes(vec![queue_scene(1, "Intro")]).await;
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
        let mut fixture =
            RecallQueueFixture::connected_with_scenes(vec![disabled, queue_scene(2, "Verse")])
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
        let (real_fade, fade_task, fade_peers) =
            crate::fade::build_engine(runtime_generation.clone(), event_bus.clone(), 1);
        fade_peers.set_lv1(lv1.clone());
        fade_task.spawn();
        let (fade_tx, mut fade_rx) = tokio::sync::mpsc::channel(8);
        let fade_proxy = FadeEngineHandle::new(fade_tx);
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
        scenes_peers.set_peers(lv1, fade_proxy);
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

    #[tokio::test]
    async fn recall_queue_shutdown_cancels_waiting_recall_without_rejecting_in_flight_reply() {
        let mut fixture = RecallQueueFixture::connected_with_scenes(vec![
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
        let second = {
            let mut fixture = RecallQueueFixture::connected_with_scenes(vec![
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

            drop(fixture);
            second
        };

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
        let mut fixture = RecallQueueFixture::connected_with_scenes(scenes).await;
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
            RecallQueueFixture::connected_with_scenes(vec![queue_scene(1, "Intro")]).await;

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

        fixture.set_snapshot(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![scene_entry(1, "Intro")],
            channels: vec![],
            ping_sequence: 10,
        });
        let first = fixture.send_recall(uuid::Uuid::from_u128(1)).await;
        let dispatch = fixture.next_lv1_recall().await;
        dispatch.reply(Ok(RecallSceneDispatch {
            scene_observation_sequence: 10,
        }));
        assert!(first.await.unwrap().is_ok());
        let mut waiting = Vec::new();
        for _ in 0..7 {
            waiting.push(fixture.send_recall(uuid::Uuid::from_u128(1)).await);
        }
        let ninth = fixture.send_recall(uuid::Uuid::from_u128(1)).await;

        yield_to_actor().await;
        assert_eq!(ninth.await.unwrap(), Err(AppCommandError::RecallQueueFull));
        for reply in &mut waiting {
            assert!(reply.try_recv().is_err());
        }
        assert!(fixture.try_next_lv1_recall().is_none());
    }

    async fn arm_recall_state(event_bus: &AppEventBus) {
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
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
        for _ in 0..8 {
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
        peers.set_peers(lv1, fade);
        task.spawn();

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

        let state = next_scene_state_with_name(&mut events, "Intro Renamed").await;
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
        let _ = next_scene_state_with_name(&mut events, "Intro Renamed").await;

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
            generation: 0,
            event: Lv1Event::SceneChanged(intro_scene()),
        });

        match next_scene_recall_event(&mut events).await {
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
        let fade = FadeEngineHandle::new(fade_tx);

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
            generation: 0,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;

        assert!(next_blocked_scene_recall_event(&mut events).await);

        handle.send(ScenesCommand::Shutdown).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn scene_recall_handle_sends_shutdown_command() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        let (lv1_tx, _lv1_rx) = tokio::sync::mpsc::channel(1);
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = FadeEngineHandle::new(fade_tx);

        let handle =
            build_and_spawn_scene_recall_fader(1, runtime_generation, lv1, fade, event_bus).await;

        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test]
    async fn explicit_recall_rechecks_lockout_after_fresh_lv1_state() {
        let event_bus = AppEventBus::default();
        let (show, show_task, _show_peers, mut lockout) =
            crate::show::build_show_actor(event_bus.clone());
        show_task.spawn();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        let (state_requested, state_requested_rx) = oneshot::channel();
        let (release_state, release_state_rx) = oneshot::channel();
        let recall_count = Arc::new(AtomicUsize::new(0));
        let recall_count_for_server = recall_count.clone();
        let server = tokio::spawn(async move {
            let Some(crate::lv1::Lv1Command::GetState { reply }) = lv1_rx.recv().await else {
                panic!("expected an LV1 state request");
            };
            let _ = state_requested.send(());
            let _ = release_state_rx.await;
            let _ = reply.send(Lv1StateSnapshot {
                connection: crate::lv1::ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![scene_entry(1, "Intro")],
                channels: Vec::new(),
                ping_sequence: 0,
            });
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    crate::lv1::Lv1Command::RecallScene { reply, .. } => {
                        recall_count_for_server.fetch_add(1, Ordering::SeqCst);
                        let _ = reply.unwrap().send(Ok(RecallSceneDispatch {
                            scene_observation_sequence: 1,
                        }));
                    }
                    _ => panic!("unexpected LV1 command"),
                }
            }
        });
        let runtime_generation = RuntimeGeneration::new();
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let scene_events = event_bus.subscribe();
        let (scenes, scenes_task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus,
            scene_events,
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            lockout.clone(),
        );
        peers.set_peers(crate::lv1::test_actor_handle(lv1_tx), fade);
        scenes_task.spawn();
        install_scene_document(&scenes, intro_scene_document()).await;

        let (reply, rx) = oneshot::channel();
        scenes
            .send(ScenesCommand::RecallScene {
                internal_scene_id: intro_internal_scene_id(),
                reply,
            })
            .await
            .unwrap();
        state_requested_rx.await.unwrap();

        let (lockout_reply, lockout_rx) = oneshot::channel();
        show.send(crate::show::ShowCommand::SetLockout {
            enabled: true,
            reply: Some(lockout_reply),
        })
        .await
        .unwrap();
        lockout_rx.await.unwrap();
        assert!(lockout.changed().await.unwrap());

        release_state.send(()).unwrap();
        assert!(matches!(
            rx.await.unwrap(),
            Err(AppCommandError::CommandFailed(message)) if message == "Recall blocked: lockout is enabled"
        ));
        assert_eq!(recall_count.load(Ordering::SeqCst), 0);

        scenes.send(ScenesCommand::Shutdown).await.unwrap();
        drop(peers);
        drop(scenes);
        drop(show);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn store_scene_config_from_current_lv1_publishes_state_change() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
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
        peers.set_peers(lv1, fade);
        task.spawn();

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::ReplaceSceneDocument {
                document: SceneDocument {
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
                reason: ScenesProjectionReason::SceneState,
                persisted_scene_edit: false,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), ScenesCommandResult { changed: true });

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
                assert_eq!(
                    state.scene_configs[0].scoped_channels,
                    vec![ChannelRef {
                        group: 0,
                        channel: 2,
                    }]
                );
                assert!(!state.scene_configs[0].scope_toggles.faders);
                assert!(state.scene_configs[0].scope_toggles.pan);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn select_scene_config_publish_persisted_scene_edits() {
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

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::ReplaceSceneDocument {
                document: SceneDocument {
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
                reason: ScenesProjectionReason::SceneState,
                persisted_scene_edit: false,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), ScenesCommandResult { changed: true });

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
        assert!(select_event);
    }

    #[tokio::test(start_paused = true)]
    async fn copy_and_paste_scene_settings_publish_only_changed_projection_state() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        let (pending_scene_observed, pending_scene_ready) = oneshot::channel();
        let (handle, task, _peers) = build_scenes_actor_with_pending_scene_observer(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            test_lockout_reader(),
            pending_scene_observed,
        );
        task.spawn();

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

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::ReplaceSceneDocument {
                document: SceneDocument {
                    scene_configs: vec![source.clone(), destination, other_source.clone()],
                    selected_scene_internal_id: None,
                },
                reason: ScenesProjectionReason::FileReplacement,
                persisted_scene_edit: false,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), ScenesCommandResult { changed: true });

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
        let (persisted_scene_edit, state) = next_scene_state_change(&mut events).await;
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
        assert_no_scene_state_change(&mut events).await;

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
        let (persisted_scene_edit, state) = next_scene_state_change(&mut events).await;
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
        assert_no_scene_state_change(&mut events).await;

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
        let (persisted_scene_edit, state) = next_scene_state_change(&mut events).await;
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
        assert_no_scene_state_change(&mut events).await;

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::CopySceneSettings {
                source_internal_scene_id: uuid::Uuid::from_u128(4),
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), Err("Scene config not found".to_string()));
        assert_no_scene_state_change(&mut events).await;

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::PasteSceneSettings {
                destination_internal_scene_id: uuid::Uuid::from_u128(4),
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), Err("Scene config not found".to_string()));
        assert_no_scene_state_change(&mut events).await;

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
        assert_no_scene_state_change(&mut events).await;

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
        assert_no_scene_state_change(&mut events).await;

        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::ReplaceSceneDocument {
                document: SceneDocument::empty(),
                reason: ScenesProjectionReason::FileReplacement,
                persisted_scene_edit: false,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), ScenesCommandResult { changed: true });
        let (persisted_scene_edit, state) = next_scene_state_change(&mut events).await;
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
            generation: 0,
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
            generation: 0,
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
            generation: 0,
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
        peers.set_peers(lv1, fade);
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
        peers.set_peers(lv1, fade);
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
        peers.set_peers(lv1, fade);
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
        peers.set_peers(lv1, fade);
        task.spawn();
        install_scene_document(&handle, intro_scene_document()).await;
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;

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
        peers.set_peers(lv1, fade);
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
        arm_recall_state(&event_bus).await;
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
        peers.set_peers(lv1, fade);
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
        let (fade, mut fade_rx, _fade_starts) = fake_fade_handle();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            events,
            settings_handle,
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers(lv1, fade);
        task.spawn();
        captured
            .wait_for_matching("event_subscriber_lagged", tracing::Level::DEBUG, |event| {
                event.fields.get("subscriber").map(String::as_str) == Some("scene-recall")
            })
            .await;
        install_scene_document(&handle, intro_scene_document()).await;
        release_lv1.send(()).unwrap();
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        captured
            .wait_for_matching("scene_recall_skipped", tracing::Level::DEBUG, |event| {
                event.fields.get("reason").map(String::as_str)
                    == Some("scene not accepted by recall policy")
            })
            .await;
        tokio::time::advance(Duration::from_millis(2_550)).await;
        yield_to_actor().await;
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
            SettingsHandle::new(settings_tx),
            AppSettings::default(),
            test_lockout_reader(),
        );
        peers.set_peers(lv1, fade);
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
            generation: 0,
            event: Lv1Event::SceneChanged(song_3_at(4)),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneListChanged(scene_list_before_current_move()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        yield_to_actor().await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneListChanged(scene_list_after_current_move()),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
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
            generation: 0,
            event: Lv1Event::SceneChanged(song_3_at(4)),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneListChanged(scene_list_before_non_current_rename()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        yield_to_actor().await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneListChanged(scene_list_after_non_current_rename()),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
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
            generation: 0,
            event: Lv1Event::SceneChanged(song_3_at(4)),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneListChanged(scene_list_before_current_move()),
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(2_050)).await;
        tokio::task::yield_now().await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneChanged(song_3_at(3)),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
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
            generation: 0,
            event: Lv1Event::SceneListChanged(scene_list_before_current_move()),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneListChanged(scene_list_before_current_move()),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
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
            generation: 0,
            event: Lv1Event::SceneListChanged(scene_list_before_non_current_rename()),
        });
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneListChanged(scene_list_after_non_current_rename()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(500)).await;
        yield_to_actor().await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;

        let mut seen_ready = false;
        let mut seen_start_requested = false;
        for _ in 0..2 {
            match next_app_event(&mut events).await {
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
            generation: 0,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;
        tokio::time::advance(Duration::from_secs(2)).await;
        yield_to_actor().await;

        match next_scene_recall_event(&mut events).await {
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
    async fn arming_and_repeat_behavior() {
        let mut state = ScenesState::default();
        let scene = intro_scene().scene;

        assert!(!state.accepts(&scene, std::time::Duration::from_millis(500)));
        assert!(!state.accepts(&scene, std::time::Duration::from_millis(500)));
        tokio::time::advance(Duration::from_secs(2)).await;
        assert!(state.accepts(&scene, std::time::Duration::from_millis(500)));
        assert!(!state.accepts(&scene, std::time::Duration::from_millis(500)));
        tokio::time::advance(Duration::from_millis(500)).await;
        assert!(state.accepts(&scene, std::time::Duration::from_millis(500)));
    }

    #[tokio::test(start_paused = true)]
    async fn empty_default_config_recall_skips_without_starting_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade_tx, mut fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = FadeEngineHandle::new(fade_tx);
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
            generation: 0,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        match next_scene_recall_event(&mut events).await {
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
                    generation: 0,
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

    async fn next_app_event(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> AppEvent {
        loop {
            let event = events.recv().await.unwrap();
            match event {
                AppEvent::Scenes {
                    generation: 1,
                    event: ScenesEvent::StateChanged { .. },
                } => continue,
                AppEvent::Scenes { generation: 1, .. } => return event,
                _ => continue,
            }
        }
    }

    async fn next_scene_recall_event(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    ) -> ScenesEvent {
        loop {
            if let AppEvent::Scenes {
                generation: 1,
                event,
            } = events.recv().await.unwrap()
                && !matches!(event, ScenesEvent::StateChanged { .. })
            {
                break event;
            }
        }
    }

    async fn next_scene_state_changed(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    ) -> crate::scenes::ScenesProjectionState {
        loop {
            if let AppEvent::Scenes {
                generation: 1,
                event: ScenesEvent::StateChanged { state, .. },
            } = events.recv().await.unwrap()
            {
                break state;
            }
        }
    }

    async fn next_scene_state_change(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    ) -> (bool, crate::scenes::ScenesProjectionState) {
        loop {
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
                break (persisted_scene_edit, state);
            }
        }
    }

    async fn assert_no_scene_state_change(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
        tokio::task::yield_now().await;
        while let Ok(event) = events.try_recv() {
            assert!(
                !matches!(
                    event,
                    AppEvent::Scenes {
                        generation: 1,
                        event: ScenesEvent::StateChanged { .. }
                    }
                ),
                "unexpected scene state change: {event:?}"
            );
        }
    }

    async fn next_scene_state_with_name(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
        scene_name: &str,
    ) -> crate::scenes::ScenesProjectionState {
        loop {
            let state = next_scene_state_changed(events).await;
            if state
                .scene_configs
                .iter()
                .any(|scene| scene.scene_name == scene_name)
            {
                break state;
            }
        }
    }

    async fn next_blocked_scene_recall_event(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    ) -> bool {
        for _ in 0..3 {
            if matches!(
                next_scene_recall_event(events).await,
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
        (FadeEngineHandle::new(command_tx), seen_rx, starts)
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
                        Some(FadeCommand::RecallSceneFade { config, expected_generation, readiness: request, reply, .. }) => {
                            let _ = seen_tx.send(QueueFadeCommand::Recall { duration_ms: config.duration_ms }).await;
                            if let Some(reply) = reply {
                                let _ = reply.send(Ok(()));
                            }
                            readiness = request.completion.map(|completion| (expected_generation.unwrap_or(1), 0, 0, completion));
                        }
                        Some(FadeCommand::WaitForRecallReadiness { scene: _, expected_generation, readiness: request, reply }) => {
                            let _ = seen_tx.send(QueueFadeCommand::Wait).await;
                            if let Some(reply) = reply {
                                let _ = reply.send(Ok(()));
                            }
                            readiness = request.completion.map(|completion| (expected_generation, 0, 0, completion));
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
        (FadeEngineHandle::new(command_tx), seen_rx)
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
        peers.set_peers(lv1, fade);
        task.spawn();
        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::ReplaceSceneDocument {
                document,
                reason: ScenesProjectionReason::FileReplacement,
                persisted_scene_edit: false,
                reply: Some(reply),
            })
            .await
            .unwrap();
        let _ = rx.await;
        handle
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
        SettingsHandle::new(tx)
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
        SettingsHandle::new(tx)
    }

    fn fake_settings_handle_then_unavailable(settings: AppSettings) -> SettingsHandle {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            if let Some(SettingsCommand::GetSettings { reply }) = rx.recv().await {
                let _ = reply.send(settings);
            }
        });
        SettingsHandle::new(tx)
    }

    async fn install_scene_document(handle: &ScenesHandle, document: SceneDocument) {
        let (reply, rx) = oneshot::channel();
        handle
            .send(ScenesCommand::ReplaceSceneDocument {
                document,
                reason: ScenesProjectionReason::FileReplacement,
                persisted_scene_edit: false,
                reply: Some(reply),
            })
            .await
            .unwrap();
        let _ = rx.await;
    }
}

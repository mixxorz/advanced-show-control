use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};

use crate::fade::{FadeCommand, FadeEngineHandle};
use crate::lv1::{
    ConnectionStatus, Lv1ActorError, Lv1ActorHandle, Lv1Command, Lv1Event, Lv1StateSnapshot,
    SceneState,
};
use crate::runtime::errors::AppCommandError;
use crate::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use crate::runtime::generation::RuntimeGeneration;
use crate::scenes::handle::ScenesHandle;
use crate::scenes::policy::{RecallPolicyDecision, RecallPolicyInput, decide_scene_recall};
use crate::scenes::{
    CueSceneResult, RecallSceneResult, ScenesCommand, ScenesCommandResult, ScenesEvent,
    ScenesProjectionReason, ScenesState, SelectedSceneResult,
};
use crate::show::{ShowCommand, ShowStateHandle};

const SCENE_CHANGED_SETTLE_DELAY: std::time::Duration = std::time::Duration::from_millis(25);

#[derive(Clone, Default)]
pub struct ScenesPeers {
    peers: Arc<Mutex<Option<ScenesPeerHandles>>>,
}

#[derive(Clone)]
struct ScenesPeerHandles {
    show: ShowStateHandle,
    lv1: Lv1ActorHandle,
    fade: FadeEngineHandle,
}

impl ScenesPeers {
    pub fn set_peers(&self, show: ShowStateHandle, lv1: Lv1ActorHandle, fade: FadeEngineHandle) {
        *self.peers.lock().expect("scene recall peer lock poisoned") =
            Some(ScenesPeerHandles { show, lv1, fade });
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
    scene: SceneState,
    seen_at: tokio::time::Instant,
    settle_after: tokio::time::Instant,
}

impl PendingSceneObservation {
    fn new(scene: SceneState, now: tokio::time::Instant) -> Self {
        Self {
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
    command_rx: mpsc::Receiver<ScenesCommand>,
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
) -> (ScenesHandle, ScenesTask, ScenesPeers) {
    let (command_tx, command_rx) = mpsc::channel(8);

    let handle = ScenesHandle::new(command_tx);
    let peers = ScenesPeers::default();
    let task = ScenesTask {
        generation,
        runtime_generation,
        peers: peers.clone(),
        events: event_bus.subscribe(),
        event_bus,
        command_rx,
    };
    (handle, task, peers)
}

async fn run_scenes_actor(task: ScenesTask) {
    let ScenesTask {
        generation,
        runtime_generation,
        peers,
        event_bus,
        mut events,
        mut command_rx,
    } = task;

    let mut recall_state = ScenesState::default();
    let mut pending_scene: Option<PendingSceneObservation> = None;

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
    // - 500 ms repeat delay:  Prevents the same scene from triggering two consecutive recalls
    //                         if a bounce or duplicate event arrives.
    loop {
        if let Some(deadline) = pending_scene.as_ref().map(|pending| pending.settle_after) {
            tokio::select! {
                command = command_rx.recv() => {
                    match command {
                        Some(ScenesCommand::GetSceneDocument { reply }) => { let _ = reply.send(recall_state.snapshot()); }
                        Some(ScenesCommand::GetSceneConfig { internal_scene_id, reply }) => { let _ = reply.send(recall_state.get_scene_config(internal_scene_id)); }
                        Some(ScenesCommand::InitialProjectionState { reply }) => { let _ = reply.send(recall_state.projection_state()); }
                        Some(ScenesCommand::SetSceneDuration { internal_scene_id, duration_ms, reply }) => {
                            let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.set_scene_duration_ms(internal_scene_id, duration_ms), &event_bus, generation);
                            if let Some(reply) = reply { let _ = reply.send(result); }
                        }
                        Some(ScenesCommand::SetSceneScopeFadersEnabled { internal_scene_id, enabled, reply }) => {
                            let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.set_scene_scope_faders_enabled(internal_scene_id, enabled), &event_bus, generation);
                            if let Some(reply) = reply { let _ = reply.send(result); }
                        }
                        Some(ScenesCommand::SetSceneScopePanEnabled { internal_scene_id, enabled, reply }) => {
                            let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.set_scene_scope_pan_enabled(internal_scene_id, enabled), &event_bus, generation);
                            if let Some(reply) = reply { let _ = reply.send(result); }
                        }
                        Some(ScenesCommand::LinkSceneConfig { source_internal_scene_id, target_scene_index, overwrite_existing, reply }) => {
                            let target = crate::lv1::SceneListEntry { index: target_scene_index, name: String::new() };
                            let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.link_scene_config(source_internal_scene_id, &target, overwrite_existing), &event_bus, generation);
                            if let Some(reply) = reply { let _ = reply.send(result); }
                        }
                        Some(ScenesCommand::DeleteSceneConfig { internal_scene_id, reply }) => {
                            let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.delete_scene_config(internal_scene_id), &event_bus, generation);
                            if let Some(reply) = reply { let _ = reply.send(result); }
                        }
                        Some(ScenesCommand::SetChannelScoped { internal_scene_id, group, channel, scoped, reply }) => {
                            let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.set_channel_scoped(internal_scene_id, group, channel, scoped), &event_bus, generation);
                            if let Some(reply) = reply { let _ = reply.send(result); }
                        }
                        Some(ScenesCommand::SetAllChannelsScoped { internal_scene_id, scoped, reply }) => {
                            let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.set_all_channels_scoped(internal_scene_id, scoped), &event_bus, generation);
                            if let Some(reply) = reply { let _ = reply.send(result); }
                        }
                        Some(ScenesCommand::CueScene { internal_scene_id, reply }) => {
                            let result = recall_state.cue_scene(internal_scene_id).map(|changed| CueSceneResult { changed, scene: recall_state.get_scene_config(internal_scene_id).unwrap() });
                            if let Some(reply) = reply { let _ = reply.send(result); }
                        }
                        Some(ScenesCommand::SelectSceneConfig { internal_scene_id, reply }) => {
                            let result = recall_state.select_scene_config(internal_scene_id).map(|_| SelectedSceneResult { scene: recall_state.get_scene_config(internal_scene_id).unwrap() });
                            if let Some(reply) = reply { let _ = reply.send(result); }
                        }
                        Some(ScenesCommand::StoreSceneConfigFromCurrentLv1 { internal_scene_id, reply }) => { let _ = internal_scene_id; if let Some(reply) = reply { let _ = reply.send(Ok(ScenesCommandResult { changed: false })); } }
                        Some(ScenesCommand::ReplaceSceneDocument { document, selected_scene_internal_id, reason, persisted_scene_edit, reply }) => {
                            recall_state.replace_snapshot(document);
                            recall_state.selected_scene_internal_id = selected_scene_internal_id;
                            publish_scene_state_changed(&event_bus, generation, reason, &recall_state, persisted_scene_edit);
                            if let Some(reply) = reply { let _ = reply.send(ScenesCommandResult { changed: true }); }
                        }
                        Some(ScenesCommand::RecallScene { internal_scene_id, reply }) => { let peer_handles = peers.handles(); let _ = reply.send(handle_explicit_recall_scene(&peer_handles.show, &peer_handles.lv1, internal_scene_id).await); }
                        Some(ScenesCommand::Shutdown) | None => break,
                    }
                }
                event = events.recv() => {
                    match event {
                        Ok(AppEvent::Lv1 { event: Lv1Event::SceneListChanged(scene_list), .. }) => {
                            recall_state.observe_scene_list(scene_list, tokio::time::Instant::now());
                        }
                        Ok(AppEvent::Lv1 { event: Lv1Event::SceneChanged(scene), .. }) => {
                            pending_scene = Some(PendingSceneObservation::new(scene, tokio::time::Instant::now()));
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                            log_lagged_subscriber("scene-recall", count);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    if let Some(observation) = pending_scene.take() {
                        let peer_handles = peers.handles();
                        process_scene_observation(
                            generation,
                            &runtime_generation,
                            &peer_handles.show,
                            &peer_handles.lv1,
                            &peer_handles.fade,
                            &event_bus,
                            &mut recall_state,
                            observation,
                        ).await;
                    }
                }
            }
            continue;
        }

        tokio::select! {
            command = command_rx.recv() => {
                    match command {
                    Some(ScenesCommand::GetSceneDocument { reply }) => { let _ = reply.send(recall_state.snapshot()); }
                    Some(ScenesCommand::GetSceneConfig { internal_scene_id, reply }) => { let _ = reply.send(recall_state.get_scene_config(internal_scene_id)); }
                    Some(ScenesCommand::InitialProjectionState { reply }) => { let _ = reply.send(recall_state.projection_state()); }
                    Some(ScenesCommand::SetSceneDuration { internal_scene_id, duration_ms, reply }) => { let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.set_scene_duration_ms(internal_scene_id, duration_ms), &event_bus, generation); if let Some(reply) = reply { let _ = reply.send(result); } }
                    Some(ScenesCommand::SetSceneScopeFadersEnabled { internal_scene_id, enabled, reply }) => { let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.set_scene_scope_faders_enabled(internal_scene_id, enabled), &event_bus, generation); if let Some(reply) = reply { let _ = reply.send(result); } }
                    Some(ScenesCommand::SetSceneScopePanEnabled { internal_scene_id, enabled, reply }) => { let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.set_scene_scope_pan_enabled(internal_scene_id, enabled), &event_bus, generation); if let Some(reply) = reply { let _ = reply.send(result); } }
                    Some(ScenesCommand::LinkSceneConfig { source_internal_scene_id, target_scene_index, overwrite_existing, reply }) => { let target = crate::lv1::SceneListEntry { index: target_scene_index, name: String::new() }; let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.link_scene_config(source_internal_scene_id, &target, overwrite_existing), &event_bus, generation); if let Some(reply) = reply { let _ = reply.send(result); } }
                    Some(ScenesCommand::DeleteSceneConfig { internal_scene_id, reply }) => { let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.delete_scene_config(internal_scene_id), &event_bus, generation); if let Some(reply) = reply { let _ = reply.send(result); } }
                    Some(ScenesCommand::SetChannelScoped { internal_scene_id, group, channel, scoped, reply }) => { let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.set_channel_scoped(internal_scene_id, group, channel, scoped), &event_bus, generation); if let Some(reply) = reply { let _ = reply.send(result); } }
                    Some(ScenesCommand::SetAllChannelsScoped { internal_scene_id, scoped, reply }) => { let result = mutate_scene_state(&mut recall_state, ScenesProjectionReason::SceneState, false, |state| state.set_all_channels_scoped(internal_scene_id, scoped), &event_bus, generation); if let Some(reply) = reply { let _ = reply.send(result); } }
                    Some(ScenesCommand::CueScene { internal_scene_id, reply }) => { let result = recall_state.cue_scene(internal_scene_id).map(|changed| CueSceneResult { changed, scene: recall_state.get_scene_config(internal_scene_id).unwrap() }); if let Some(reply) = reply { let _ = reply.send(result); } }
                    Some(ScenesCommand::SelectSceneConfig { internal_scene_id, reply }) => { let result = recall_state.select_scene_config(internal_scene_id).map(|_| SelectedSceneResult { scene: recall_state.get_scene_config(internal_scene_id).unwrap() }); if let Some(reply) = reply { let _ = reply.send(result); } }
                    Some(ScenesCommand::StoreSceneConfigFromCurrentLv1 { internal_scene_id, reply }) => { let _ = internal_scene_id; if let Some(reply) = reply { let _ = reply.send(Ok(ScenesCommandResult { changed: false })); } }
                    Some(ScenesCommand::ReplaceSceneDocument { document, selected_scene_internal_id, reason, persisted_scene_edit, reply }) => { recall_state.replace_snapshot(document); recall_state.selected_scene_internal_id = selected_scene_internal_id; publish_scene_state_changed(&event_bus, generation, reason, &recall_state, persisted_scene_edit); if let Some(reply) = reply { let _ = reply.send(ScenesCommandResult { changed: true }); } }
                    Some(ScenesCommand::RecallScene { internal_scene_id, reply }) => { let peer_handles = peers.handles(); let _ = reply.send(handle_explicit_recall_scene(&peer_handles.show, &peer_handles.lv1, internal_scene_id).await); }
                    Some(ScenesCommand::Shutdown) | None => break,
                }
            }
            event = events.recv() => {
                match event {
                    Ok(AppEvent::Lv1 {
                        event: Lv1Event::SceneListChanged(scene_list),
                        ..
                    }) => {
                        recall_state.observe_scene_list(scene_list, tokio::time::Instant::now());
                    }
                    Ok(AppEvent::Lv1 {
                        event: Lv1Event::SceneChanged(scene),
                        ..
                    }) => {
                        pending_scene = Some(PendingSceneObservation::new(
                            scene,
                            tokio::time::Instant::now(),
                        ));
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        log_lagged_subscriber("scene-recall", count);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
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

#[allow(clippy::too_many_arguments)]
async fn process_scene_observation(
    generation: u64,
    runtime_generation: &RuntimeGeneration,
    show: &ShowStateHandle,
    lv1: &Lv1ActorHandle,
    fade: &FadeEngineHandle,
    event_bus: &AppEventBus,
    recall_state: &mut ScenesState,
    observation: PendingSceneObservation,
) {
    let now = tokio::time::Instant::now();
    if recall_state.is_scene_list_edit_suppressed(observation.seen_at)
        || recall_state.is_scene_list_edit_suppressed(now)
    {
        let scene_label = scene_label(&observation.scene);
        let reason = "scene list edit suppression";
        tracing::debug!(event = "scene_recall_skipped", scene = %scene_label, reason = %reason, "Scene recall skipped for {scene_label}: {reason}");
        return;
    }
    if !recall_state.accepts(&observation.scene) {
        let scene_label = scene_label(&observation.scene);
        let reason = "scene not accepted by recall policy";
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

    let (reply, rx) = oneshot::channel();
    if show
        .send(ShowCommand::GetShowDocument { reply })
        .await
        .is_err()
    {
        if !is_generation_current(generation, runtime_generation).await {
            return;
        }
        event_bus.publish_scenes(
            generation,
            ScenesEvent::Blocked {
                scene_label: scene_label(&observation.scene),
                reason: "failed to fetch show document: show state is unavailable".to_string(),
            },
        );
        return;
    }
    let show_document = match rx.await {
        Ok(show_document) => show_document,
        Err(_) => {
            if !is_generation_current(generation, runtime_generation).await {
                return;
            }
            event_bus.publish_scenes(
                generation,
                ScenesEvent::Blocked {
                    scene_label: scene_label(&observation.scene),
                    reason: "failed to fetch show document: reply channel closed".to_string(),
                },
            );
            return;
        }
    };
    let lockout = show_document.lockout;
    let scene_config = show_document.scene_configs.into_iter().find(|scene| {
        scene.scene_index == Some(observation.scene.index)
            && scene.scene_name == observation.scene.name
    });

    match decide_scene_recall(RecallPolicyInput {
        recalled_scene: observation.scene.clone(),
        lv1_snapshot,
        lockout,
        scene_config,
    }) {
        RecallPolicyDecision::Start(fade_config) => {
            let scene_label = scene_label(&observation.scene);
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
            let result = if !is_generation_current(generation, runtime_generation).await {
                Err(AppCommandError::StaleGeneration)
            } else {
                match fade
                    .send(FadeCommand::RecallSceneFade {
                        config: fade_config,
                        expected_generation: Some(generation),
                        reply: Some(reply),
                    })
                    .await
                {
                    Ok(()) => match rx.await {
                        Ok(result) => result,
                        Err(_) => Err(AppCommandError::ReplyChannelClosed),
                    },
                    Err(_) => Err(AppCommandError::FadeUnavailable),
                }
            };
            match result {
                Ok(()) | Err(AppCommandError::StaleGeneration) => (),
                Err(err) => {
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
        }
    }
}

fn scene_label(scene: &SceneState) -> String {
    format!("{}: {}", scene.index, scene.name)
}

async fn handle_explicit_recall_scene(
    show: &ShowStateHandle,
    lv1: &Lv1ActorHandle,
    internal_scene_id: uuid::Uuid,
) -> Result<RecallSceneResult, AppCommandError> {
    tracing::debug!(
        event = "scene_recall_requested",
        internal_scene_id = %internal_scene_id,
        "Scene recall requested"
    );

    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::GetState { reply })
        .await
        .map_err(|error| match error {
            Lv1ActorError::NotConnected => AppCommandError::CommandFailed(
                "Recall blocked: LV1 state is unavailable".to_string(),
            ),
            other => AppCommandError::CommandFailed(other.to_string()),
        })
        .map_err(|error| {
            tracing::warn!(
                event = "scene_recall_blocked",
                internal_scene_id = %internal_scene_id,
                reason = %error,
                "Scene recall blocked: {error}"
            );
            error
        })?;
    let lv1_snapshot = rx
        .await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(|error| {
            tracing::warn!(
                event = "scene_recall_blocked",
                internal_scene_id = %internal_scene_id,
                reason = %error,
                "Scene recall blocked: {error}"
            );
            error
        })?;
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::GetShowDocument { reply })
        .await
        .map_err(|_| AppCommandError::ShowUnavailable)?;
    let show_document = rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?;
    let result = crate::show::validate_recall_scene_request(
        &show_document,
        &lv1_snapshot,
        internal_scene_id,
    )
    .map_err(|message| {
        tracing::warn!(
            event = "scene_recall_blocked",
            internal_scene_id = %internal_scene_id,
            reason = %message,
            "Scene recall blocked: {message}"
        );
        AppCommandError::CommandFailed(message)
    })?;

    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::RecallScene {
        scene_index: result.lv1_scene_index,
        reply: Some(reply),
    })
    .await
    .map_err(|error| match error {
        Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
        other => AppCommandError::CommandFailed(other.to_string()),
    })?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)?
        .map_err(|error| match error {
            Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
            other => AppCommandError::CommandFailed(other.to_string()),
        })?;
    tracing::debug!(
        event = "scene_recall_command_sent",
        internal_scene_id = %result.scene.internal_scene_id,
        scene_index = result.scene.scene_index,
        scene_name = %result.scene.scene_name,
        "Scene recall command sent: {}",
        result.scene.scene_name
    );
    Ok(result)
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
    use crate::lv1::{Lv1ActorHandle, Lv1Event, Lv1StateSnapshot, SceneListEntry, SceneState};
    use crate::scenes::events::ScenesEvent;
    use crate::scenes::{ChannelConfig, ChannelRef, SceneConfig, SceneScopeToggles};
    use crate::show::{ShowDocument, ShowStateHandle};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

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

    fn song_3_at(index: i32) -> SceneState {
        SceneState {
            index,
            name: "Song 3".to_string(),
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
    async fn unavailable_lv1_state_blocks_before_start() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let (lv1_tx, lv1_rx) = tokio::sync::mpsc::channel(1);
        drop(lv1_rx);
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        let (fade, _fade_rx, _fade_starts) = fake_fade_handle();
        let show = ShowStateHandle::new_empty(event_bus.clone());

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            show.clone(),
            lv1,
            fade,
            event_bus.clone(),
        );
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
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade_tx, fade_rx) = tokio::sync::mpsc::channel(1);
        drop(fade_rx);
        let fade = FadeEngineHandle::new(fade_tx);
        seed_show(&show).await;

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            show.clone(),
            lv1,
            fade,
            event_bus.clone(),
        );
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

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation,
            ShowStateHandle::new_empty(event_bus.clone()),
            lv1,
            fade,
            event_bus,
        );

        handle.send(ScenesCommand::Shutdown).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn stale_generation_does_not_start_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, _fade_rx, fade_starts) = fake_fade_handle();
        seed_show(&show).await;

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            show.clone(),
            lv1,
            fade,
            event_bus.clone(),
        );
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
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, _fade_rx, fade_starts) = fake_fade_handle();
        seed_show(&show).await;

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            show.clone(),
            lv1,
            fade,
            event_bus.clone(),
        );
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
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
        seed_show(&show).await;

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            show.clone(),
            lv1,
            fade,
            event_bus.clone(),
        );
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        yield_to_actor().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        yield_to_actor().await;

        let fade_command = next_fade_command(&mut fade_rx).await;
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
    async fn current_scene_move_sequence_does_not_start_fade() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
        seed_show(&show).await;

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            show.clone(),
            lv1,
            fade,
            event_bus.clone(),
        );
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
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
        seed_show(&show).await;

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            show.clone(),
            lv1,
            fade,
            event_bus.clone(),
        );
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
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
        seed_show(&show).await;

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            show.clone(),
            lv1,
            fade,
            event_bus.clone(),
        );
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
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
        seed_show(&show).await;

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            show.clone(),
            lv1,
            fade,
            event_bus.clone(),
        );
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

        let fade_command = next_fade_command(&mut fade_rx).await;
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
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
        seed_show(&show).await;

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            show.clone(),
            lv1,
            fade,
            event_bus.clone(),
        );
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

        let fade_command = tokio::time::timeout(Duration::from_secs(1), fade_rx.recv())
            .await
            .unwrap()
            .unwrap();
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
        let show = show_handle();
        let (lv1, release_lv1, server) =
            spawn_fake_lv1_with_mismatched_scene(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
        seed_show(&show).await;

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            show.clone(),
            lv1,
            fade,
            event_bus.clone(),
        );
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
        let scene = intro_scene();

        assert!(!state.accepts(&scene));
        assert!(!state.accepts(&scene));
        tokio::time::advance(Duration::from_secs(2)).await;
        assert!(state.accepts(&scene));
        assert!(!state.accepts(&scene));
        tokio::time::advance(Duration::from_millis(500)).await;
        assert!(state.accepts(&scene));
    }

    #[tokio::test(start_paused = true)]
    async fn skipped_recall_does_not_abort_existing_fade() {
        let event_bus = AppEventBus::default();
        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let show = show_handle();
        let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
        let (fade, mut fade_rx, fade_starts) = fake_fade_handle();
        seed_show_with_duration(&show, 0).await;
        let (reply, rx) = oneshot::channel();
        show.send(ShowCommand::SetSceneScopeFadersEnabled {
            internal_scene_id: intro_internal_scene_id(),
            enabled: false,
            reply: Some(reply),
        })
        .await
        .unwrap();
        let _ = rx.await.unwrap().unwrap();

        let handle = build_and_spawn_scene_recall_fader(
            1,
            runtime_generation.clone(),
            show.clone(),
            lv1,
            fade,
            event_bus.clone(),
        );
        release_lv1.send(()).unwrap();
        arm_recall_state(&event_bus).await;

        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: Lv1Event::SceneChanged(intro_scene()),
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        assert!(matches!(
            fade_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        assert_eq!(fade_starts.load(Ordering::SeqCst), 0);

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
                    event: _,
                } => return event,
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
            {
                break event;
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

    async fn next_fade_command(
        fade_rx: &mut tokio::sync::mpsc::Receiver<FadeConfig>,
    ) -> FadeConfig {
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
                scene: Some(intro_scene()),
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
                        let _ = reply.unwrap().send(Ok(()));
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
                        let _ = reply.unwrap().send(Ok(()));
                    }
                    crate::lv1::Lv1Command::Flush { reply } => {
                        let _ = reply.unwrap().send(Ok(()));
                    }
                }
            }
        });
        (crate::lv1::test_actor_handle(lv1_tx), release_tx, server)
    }

    fn show_handle() -> ShowStateHandle {
        ShowStateHandle::new_empty(AppEventBus::default())
    }

    async fn seed_show(handle: &ShowStateHandle) {
        seed_show_with_duration(handle, 4_000).await;
    }

    async fn seed_show_with_duration(handle: &ShowStateHandle, duration_ms: u64) {
        let snapshot = ShowDocument {
            lockout: false,
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
                scope_toggles: SceneScopeToggles::default(),
            }],
            cued_scene_internal_id: None,
        };
        let (reply, rx) = oneshot::channel();
        handle
            .send(ShowCommand::ReplaceSnapshotForTest {
                snapshot,
                reply: Some(reply),
            })
            .await
            .unwrap();
        let _ = rx.await;
    }

    fn intro_internal_scene_id() -> uuid::Uuid {
        uuid::Uuid::from_u128(0x11111111111141118111111111111111)
    }

    fn fake_fade_handle() -> (
        FadeEngineHandle,
        tokio::sync::mpsc::Receiver<FadeConfig>,
        Arc<AtomicUsize>,
    ) {
        let (command_tx, mut command_rx) = tokio::sync::mpsc::channel(8);
        let (seen_tx, seen_rx) = tokio::sync::mpsc::channel(8);
        let starts = Arc::new(AtomicUsize::new(0));
        let starts_clone = starts.clone();
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                if let FadeCommand::RecallSceneFade { config, reply, .. } = command {
                    let _ = seen_tx.send(config.clone()).await;
                    starts_clone.fetch_add(1, Ordering::SeqCst);
                    let _ = reply.unwrap().send(Ok(()));
                }
            }
        });
        (FadeEngineHandle::new(command_tx), seen_rx, starts)
    }

    fn intro_scene() -> SceneState {
        SceneState {
            index: 1,
            name: "Intro".to_string(),
        }
    }

    fn build_and_spawn_scene_recall_fader(
        generation: u64,
        runtime_generation: RuntimeGeneration,
        show: ShowStateHandle,
        lv1: Lv1ActorHandle,
        fade: FadeEngineHandle,
        event_bus: AppEventBus,
    ) -> ScenesHandle {
        let (handle, task, peers) = build_scenes_actor(generation, runtime_generation, event_bus);
        peers.set_peers(show, lv1, fade);
        task.spawn();
        handle
    }
}

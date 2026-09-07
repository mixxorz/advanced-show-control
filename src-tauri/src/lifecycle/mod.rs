//! App runtime lifecycle ownership.

use std::sync::Arc;

#[cfg(test)]
use std::future::Future;
#[cfg(test)]
use std::pin::Pin;
use tauri::{AppHandle, Runtime};
use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;
use tracing::instrument::WithSubscriber;

#[cfg(test)]
use crate::cue_lists::CueListsPeers;
use crate::cue_lists::{CueListsHandle, build_cue_lists_actor};
use crate::fade::{FadeEngineHandle, build_engine};
use crate::logging::UiLogEvent;
use crate::lv1::{ConnectionStatus, Lv1ActorHandle, Lv1Command, Lv1Event, build_actor};
use crate::runtime::errors::AppCommandError;
use crate::runtime::events::{AppEvent, AppEventBus};
use crate::runtime::generation::RuntimeGeneration;
use crate::scenes::{ScenesHandle, ScenesPeers, build_scenes_actor};
use crate::settings::{SettingsCommand, SettingsHandle};
use crate::show::{
    ConnectCommandResult, ShowActorPeers, ShowCommand, ShowCommandResult, ShowLockoutReader,
    ShowStateHandle,
};

#[derive(Default)]
pub struct RuntimeHandles {
    pub lv1: Option<Lv1ActorHandle>,
    pub fade: Option<FadeEngineHandle>,
}

impl RuntimeHandles {
    pub fn with_runtime_targets(lv1: Lv1ActorHandle, fade: FadeEngineHandle) -> Self {
        Self {
            lv1: Some(lv1),
            fade: Some(fade),
        }
    }

    pub fn abort_all(&mut self) {
        self.lv1 = None;
        self.fade = None;
    }
}

pub enum RuntimeInstallRejection {
    StaleGeneration { handles: RuntimeHandles },
    MissingRuntimeTargets { handles: RuntimeHandles },
}

#[cfg(test)]
type BeforeConnectionMetadataHook =
    Box<dyn FnOnce(RuntimeGeneration) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

#[cfg(test)]
type DisconnectTestHook = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

struct BuiltConnectedRuntime {
    lv1: Lv1ActorHandle,
    lv1_task: crate::lv1::Lv1ActorTask,
    fade: FadeEngineHandle,
    fade_task: crate::fade::FadeEngineTask,
}

impl BuiltConnectedRuntime {
    fn runtime_targets(&self) -> RuntimeHandles {
        RuntimeHandles::with_runtime_targets(self.lv1.clone(), self.fade.clone())
    }

    fn spawn_lv1_and_fade(self) -> StartedConnectedRuntime {
        self.lv1_task.spawn();
        self.fade_task.spawn();
        StartedConnectedRuntime {
            lv1: self.lv1,
            fade: self.fade,
            #[cfg(test)]
            before_connection_metadata: None,
        }
    }
}

struct StartedConnectedRuntime {
    lv1: Lv1ActorHandle,
    fade: FadeEngineHandle,
    #[cfg(test)]
    before_connection_metadata: Option<BeforeConnectionMetadataHook>,
}

fn build_connected_runtime(
    generation: u64,
    runtime_generation: RuntimeGeneration,
    identity: &crate::connection_state::Lv1SystemIdentity,
    event_bus: AppEventBus,
) -> BuiltConnectedRuntime {
    let (lv1, lv1_task) = build_actor(
        identity.address.clone(),
        identity.port,
        event_bus.clone(),
        generation,
    );
    let (fade, fade_task) = build_engine(
        runtime_generation.clone(),
        event_bus.clone(),
        generation,
        lv1.clone(),
    );
    BuiltConnectedRuntime {
        lv1,
        lv1_task,
        fade,
        fade_task,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeClearTransaction {
    cleared_generation: u64,
    active_generation: u64,
}

impl RuntimeInstallRejection {
    pub fn into_handles(self) -> RuntimeHandles {
        match self {
            Self::StaleGeneration { handles } | Self::MissingRuntimeTargets { handles } => handles,
        }
    }
}

struct LifecycleInner {
    generation: RuntimeGeneration,
    connecting: bool,
    frontend_ready: bool,
    handles: RuntimeHandles,
    runtime_handles_generation: Option<u64>,
    projector: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub struct RuntimeSnapshotSource {
    inner: Arc<Mutex<LifecycleInner>>,
    generation: RuntimeGeneration,
}

impl RuntimeSnapshotSource {
    pub async fn connected_lv1(&self) -> Option<(u64, Lv1ActorHandle)> {
        let inner = self.inner.lock().await;
        let generation = inner.generation.current().await;
        if inner.runtime_handles_generation != Some(generation) {
            return None;
        }
        inner.handles.lv1.clone().map(|lv1| (generation, lv1))
    }

    pub async fn current_generation(&self) -> u64 {
        self.generation.current().await
    }
}

#[derive(Clone)]
pub struct AppLifecycle {
    inner: Arc<Mutex<LifecycleInner>>,
    event_bus: AppEventBus,
    show: ShowStateHandle,
    show_peers: ShowActorPeers,
    #[cfg(test)]
    lockout: ShowLockoutReader,
    cue_lists: CueListsHandle,
    #[cfg(test)]
    cue_lists_peers: CueListsPeers,
    scenes: ScenesHandle,
    scenes_peers: ScenesPeers,
    settings: SettingsHandle,
    transition_lock: Arc<Mutex<()>>,
    discovery_lock: Arc<Mutex<()>>,
    #[cfg(test)]
    before_disconnect_cleanup: Arc<Mutex<Option<DisconnectTestHook>>>,
    #[cfg(test)]
    before_connection_success: Arc<Mutex<Option<DisconnectTestHook>>>,
}

impl AppLifecycle {
    pub fn new(
        event_bus: AppEventBus,
        show: ShowStateHandle,
        show_peers: ShowActorPeers,
        lockout: ShowLockoutReader,
        settings: SettingsHandle,
        initial_settings: crate::settings::AppSettings,
    ) -> Self {
        let (cue_lists, cue_lists_task, cue_lists_peers) = build_cue_lists_actor(event_bus.clone());
        show_peers.set_cue_lists(cue_lists.clone());
        cue_lists_task.spawn();
        let runtime_generation = show_peers.runtime_generation();
        let (scenes, scenes_task, scenes_peers) = build_scenes_actor(
            0,
            runtime_generation.clone(),
            event_bus.clone(),
            event_bus.subscribe(),
            settings.clone(),
            initial_settings,
            lockout.clone(),
        );
        show_peers.set_scenes(scenes.clone());
        cue_lists_peers.set_scenes(scenes.clone());
        scenes_task.spawn();

        Self {
            inner: Arc::new(Mutex::new(LifecycleInner {
                generation: runtime_generation,
                connecting: false,
                frontend_ready: false,
                handles: RuntimeHandles::default(),
                runtime_handles_generation: None,
                projector: None,
            })),
            event_bus,
            show,
            show_peers,
            #[cfg(test)]
            lockout,
            cue_lists,
            #[cfg(test)]
            cue_lists_peers,
            scenes,
            scenes_peers,
            settings,
            transition_lock: Arc::new(Mutex::new(())),
            discovery_lock: Arc::new(Mutex::new(())),
            #[cfg(test)]
            before_disconnect_cleanup: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            before_connection_success: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    async fn set_before_disconnect_cleanup(&self, hook: DisconnectTestHook) {
        *self.before_disconnect_cleanup.lock().await = Some(hook);
    }

    #[cfg(test)]
    async fn run_before_disconnect_cleanup(&self) {
        let hook = self.before_disconnect_cleanup.lock().await.take();
        if let Some(hook) = hook {
            hook().await;
        }
    }

    #[cfg(test)]
    async fn set_before_connection_success(&self, hook: DisconnectTestHook) {
        *self.before_connection_success.lock().await = Some(hook);
    }

    #[cfg(test)]
    async fn run_before_connection_success(&self) {
        let hook = self.before_connection_success.lock().await.take();
        if let Some(hook) = hook {
            hook().await;
        }
    }

    async fn settings_snapshot(&self) -> Result<crate::settings::AppSettings, String> {
        let (reply, rx) = oneshot::channel();
        self.settings
            .send(SettingsCommand::GetSettings { reply })
            .await
            .map_err(|_| "Settings are unavailable".to_string())?;
        rx.await
            .map_err(|_| "Settings reply channel is closed".to_string())
    }

    pub async fn begin_connecting(&self) -> Option<u64> {
        let _transition = self.transition_lock.lock().await;
        let mut inner = self.inner.lock().await;
        let generation = inner.generation.advance().await;
        inner.connecting = true;
        drop(inner);
        self.event_bus
            .publish_runtime_generation_changed(generation);
        Some(generation)
    }

    pub async fn active_generation(&self) -> u64 {
        self.inner.lock().await.generation.current().await
    }

    pub async fn install_runtime_transaction(
        &self,
        generation: u64,
        handles: RuntimeHandles,
    ) -> Result<(), RuntimeInstallRejection> {
        let mut inner = self.inner.lock().await;
        if inner.generation.current().await != generation {
            return Err(RuntimeInstallRejection::StaleGeneration { handles });
        }

        if handles.lv1.is_none() || handles.fade.is_none() {
            return Err(RuntimeInstallRejection::MissingRuntimeTargets { handles });
        }

        let lv1 = handles.lv1.clone().expect("validated LV1 handle");
        inner.handles = handles;
        inner.runtime_handles_generation = Some(generation);
        self.show_peers.set_lv1(generation, lv1);
        inner.connecting = false;
        Ok(())
    }

    #[cfg(test)]
    async fn install_accepted_scene_recall_fader(
        &self,
        generation: u64,
        _handle: ScenesHandle,
    ) -> bool {
        self.install_accepted_scene_peers(generation).await
    }

    async fn install_accepted_scene_peers(&self, generation: u64) -> bool {
        let inner = self.inner.lock().await;
        if inner.generation.current().await != generation
            || inner.runtime_handles_generation != Some(generation)
        {
            return false;
        }

        let Some(lv1) = inner.handles.lv1.clone() else {
            return false;
        };
        let Some(fade) = inner.handles.fade.clone() else {
            return false;
        };
        self.scenes_peers
            .set_peers_for_generation(generation, lv1, fade);
        true
    }

    async fn clear_runtime_if_current(
        &self,
        expected_generation: u64,
    ) -> Option<RuntimeClearTransaction> {
        let mut inner = self.inner.lock().await;
        let active_generation = inner
            .generation
            .advance_if_current(expected_generation)
            .await?;

        inner.handles.abort_all();
        inner.runtime_handles_generation = None;
        inner.connecting = false;
        self.show_peers.clear_lv1(expected_generation);
        self.scenes_peers
            .clear_peers_for_generation(expected_generation);

        Some(RuntimeClearTransaction {
            cleared_generation: expected_generation,
            active_generation,
        })
    }

    pub async fn clear_runtime_transaction(&self, generation: u64) {
        if let Some(transaction) = self.clear_runtime_if_current(generation).await {
            self.event_bus
                .publish_runtime_generation_changed(transaction.active_generation);
        }
    }

    pub async fn abort_current_runtime(&self) {
        let generation = self.active_generation().await;
        self.clear_runtime_transaction(generation).await;
    }

    async fn abort_rejected_connection_transaction(
        &self,
        generation: u64,
        mut candidate_handles: RuntimeHandles,
    ) {
        candidate_handles.abort_all();
        let mut inner = self.inner.lock().await;
        if inner.runtime_handles_generation == Some(generation) {
            inner.handles.abort_all();
            inner.runtime_handles_generation = None;
            self.scenes_peers.clear_peers_for_generation(generation);
        }
        drop(inner);
        self.show_peers.clear_lv1(generation);
    }

    pub async fn connect_to_identity<R: Runtime>(
        &self,
        app: AppHandle<R>,
        generation: u64,
        identity: crate::connection_state::Lv1SystemIdentity,
    ) -> Result<ConnectCommandResult, String> {
        log_lv1_connect_requested(&identity);
        log_lv1_connecting(&identity);
        let runtime_generation = self.current_runtime_generation().await;
        let built_runtime = build_connected_runtime(
            generation,
            runtime_generation,
            &identity,
            self.event_bus.clone(),
        );
        let handles = built_runtime.runtime_targets();
        if let Err(rejection) = self.install_runtime_transaction(generation, handles).await {
            self.abort_rejected_connection_transaction(generation, rejection.into_handles())
                .await;
            return Err("generation is stale".to_string());
        }
        let started_runtime = built_runtime.spawn_lv1_and_fade();

        let _ = app;
        let result = self.spawn_finish_connect_transaction(identity, generation, started_runtime);
        result
            .await
            .map_err(|_| "LV1 connection finalizer task was cancelled".to_string())?
    }

    fn spawn_finish_connect_transaction(
        &self,
        identity: crate::connection_state::Lv1SystemIdentity,
        generation: u64,
        started_runtime: StartedConnectedRuntime,
    ) -> oneshot::Receiver<Result<ConnectCommandResult, String>> {
        let (reply, result) = oneshot::channel();
        let lifecycle = self.clone();
        let subscriber = tracing::dispatcher::get_default(|dispatcher| dispatcher.clone());
        tauri::async_runtime::spawn(
            async move {
                let outcome = lifecycle
                    .finish_connect_transaction(identity, generation, started_runtime)
                    .await;
                let _ = reply.send(outcome);
            }
            .with_subscriber(subscriber),
        );
        result
    }

    async fn finish_connect_transaction(
        &self,
        identity: crate::connection_state::Lv1SystemIdentity,
        generation: u64,
        started_runtime: StartedConnectedRuntime,
    ) -> Result<ConnectCommandResult, String> {
        let StartedConnectedRuntime {
            lv1,
            fade,
            #[cfg(test)]
            before_connection_metadata,
        } = started_runtime;

        let (reply, rx) = oneshot::channel();
        if let Err(error) = lv1.send(Lv1Command::GetState { reply }).await {
            return self
                .finalize_failed_connection(
                    generation,
                    identity,
                    format!("Failed to request initial LV1 state: {error}"),
                    #[cfg(test)]
                    before_connection_metadata,
                )
                .await;
        }
        let initial_snapshot = match rx.await {
            Ok(snapshot) => snapshot,
            Err(_) => {
                return self
                    .finalize_failed_connection(
                        generation,
                        identity,
                        AppCommandError::ReplyChannelClosed.to_string(),
                        #[cfg(test)]
                        before_connection_metadata,
                    )
                    .await;
            }
        };

        if initial_snapshot.connection != ConnectionStatus::Connected {
            let lifecycle = self.clone();
            let subscriber = tracing::dispatcher::get_default(|dispatcher| dispatcher.clone());
            let finalizer = tauri::async_runtime::spawn(
                async move {
                    lifecycle
                        .finalize_failed_connection(
                            generation,
                            identity,
                            "LV1 did not connect".to_string(),
                            #[cfg(test)]
                            before_connection_metadata,
                        )
                        .await
                }
                .with_subscriber(subscriber),
            );
            return finalizer
                .await
                .map_err(|error| format!("LV1 failure finalizer task failed: {error}"))?;
        }

        let lifecycle = self.clone();
        let subscriber = tracing::dispatcher::get_default(|dispatcher| dispatcher.clone());
        let finalizer = tauri::async_runtime::spawn(
            async move {
                lifecycle
                    .finalize_connection_metadata(
                        generation,
                        identity,
                        lv1,
                        fade,
                        initial_snapshot,
                        #[cfg(test)]
                        before_connection_metadata,
                    )
                    .await
            }
            .with_subscriber(subscriber),
        );
        finalizer
            .await
            .map_err(|error| format!("LV1 connection finalizer task failed: {error}"))?
    }

    #[allow(clippy::too_many_arguments)]
    async fn finalize_connection_metadata(
        &self,
        generation: u64,
        identity: crate::connection_state::Lv1SystemIdentity,
        lv1: Lv1ActorHandle,
        fade: FadeEngineHandle,
        _initial_snapshot: crate::lv1::Lv1StateSnapshot,
        #[cfg(test)] before_connection_metadata: Option<BeforeConnectionMetadataHook>,
    ) -> Result<ConnectCommandResult, String> {
        #[cfg(test)]
        if let Some(before_connection_metadata) = before_connection_metadata {
            before_connection_metadata(self.current_runtime_generation().await).await;
        }

        let completion = self
            .complete_lv1_connection_metadata(generation, identity.clone())
            .await
            .map_err(|error| error.to_string())?;
        if !completion.accepted {
            self.abort_rejected_connection_transaction(
                generation,
                RuntimeHandles {
                    lv1: Some(lv1),
                    fade: Some(fade),
                },
            )
            .await;
            return Err("LV1 connection was superseded".to_string());
        }

        if !self.install_accepted_scene_peers(generation).await {
            self.abort_rejected_connection_transaction(
                generation,
                RuntimeHandles {
                    lv1: Some(lv1),
                    fade: Some(fade),
                },
            )
            .await;
            return Err("generation is stale".to_string());
        }
        let (peers_ready_reply, peers_ready_result) = tokio::sync::oneshot::channel();
        let peers_ready = self
            .scenes
            .send(crate::scenes::ScenesCommand::RuntimePeersReady {
                generation,
                initial_scene_list: _initial_snapshot.scene_list,
                reply: peers_ready_reply,
            })
            .await;
        let peers_ready_succeeded =
            matches!(peers_ready, Ok(()) if matches!(peers_ready_result.await, Ok(Ok(()))));
        if !peers_ready_succeeded {
            self.abort_rejected_connection_transaction(
                generation,
                RuntimeHandles {
                    lv1: Some(lv1),
                    fade: Some(fade),
                },
            )
            .await;
            return Err("scene actor is unavailable".to_string());
        }

        if let Err(error) = self
            .remember_last_connected_lv1(generation, identity.clone())
            .await
        {
            self.log_last_connected_lv1_save_failure(generation, error)
                .await;
        }
        let result = ConnectCommandResult {
            changed: completion.changed,
        };
        #[cfg(test)]
        self.run_before_connection_success().await;
        let accepted = self
            .current_runtime_generation()
            .await
            .if_current(generation, || {
                log_lv1_connected(&identity);
                result
            })
            .await;
        accepted.ok_or_else(|| "LV1 connection was superseded".to_string())
    }

    async fn finalize_failed_connection(
        &self,
        generation: u64,
        identity: crate::connection_state::Lv1SystemIdentity,
        error: String,
        #[cfg(test)] before_connection_metadata: Option<BeforeConnectionMetadataHook>,
    ) -> Result<ConnectCommandResult, String> {
        #[cfg(test)]
        if let Some(before_connection_metadata) = before_connection_metadata {
            before_connection_metadata(self.current_runtime_generation().await).await;
        }
        let failure = self.fail_lv1_connection_metadata(generation).await;
        if failure.as_ref().is_ok_and(|outcome| outcome.accepted) {
            let generation_guard = self.current_runtime_generation().await;
            let _ = generation_guard
                .if_current(generation, || log_lv1_connect_failed(&identity))
                .await;
        }
        self.clear_runtime_transaction(generation).await;
        Err(error)
    }

    async fn complete_lv1_connection_metadata(
        &self,
        expected_generation: u64,
        identity: crate::connection_state::Lv1SystemIdentity,
    ) -> Result<crate::show::CompleteConnectionOutcome, AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.show
            .send(ShowCommand::CompleteLv1ConnectionIfCurrent {
                identity,
                runtime_generation: self.current_runtime_generation().await,
                expected_generation,
                reply,
            })
            .await
            .map_err(|_| AppCommandError::ShowUnavailable)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)
    }

    async fn fail_lv1_connection_metadata(
        &self,
        expected_generation: u64,
    ) -> Result<crate::show::CompleteConnectionOutcome, AppCommandError> {
        let (reply, rx) = oneshot::channel();
        let command = ShowCommand::FailLv1ConnectionIfCurrent {
            runtime_generation: self.current_runtime_generation().await,
            expected_generation,
            reply,
        };
        self.show
            .send(command)
            .await
            .map_err(|_| AppCommandError::ShowUnavailable)?;
        rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)
    }

    pub async fn disconnect_current_runtime(&self) -> Result<ShowCommandResult, String> {
        tracing::debug!(
            event = "lv1_disconnect_requested",
            "LV1 disconnect requested"
        );
        let generation = self.active_generation().await;
        self.disconnect_runtime_generation(generation).await
    }

    async fn disconnect_runtime_generation(
        &self,
        generation: u64,
    ) -> Result<ShowCommandResult, String> {
        #[cfg(test)]
        self.run_before_disconnect_cleanup().await;
        let _transition = self.transition_lock.lock().await;
        self.finish_disconnect(generation).await
    }

    fn superseded_disconnect(&self, generation: u64) -> ShowCommandResult {
        tracing::debug!(
            event = "lv1_disconnect_superseded",
            generation,
            "Disconnect request was superseded by a newer LV1 runtime"
        );
        ShowCommandResult { changed: false }
    }

    async fn clear_lv1_connection_metadata(
        &self,
        expected_generation: u64,
    ) -> Result<crate::show::CompleteConnectionOutcome, String> {
        let (reply, rx) = oneshot::channel();
        self.show
            .send(ShowCommand::ClearLv1ConnectionIfCurrent {
                runtime_generation: self.current_runtime_generation().await,
                expected_generation,
                reply,
            })
            .await
            .map_err(|_| "Show state is unavailable".to_string())?;
        rx.await
            .map_err(|_| "Show state reply channel is closed".to_string())
    }

    async fn finish_disconnect(&self, generation: u64) -> Result<ShowCommandResult, String> {
        let cleared = self.clear_lv1_connection_metadata(generation).await?;
        if !cleared.accepted {
            return Ok(self.superseded_disconnect(generation));
        }
        let Some(transaction) = self.clear_runtime_if_current(generation).await else {
            return Ok(self.superseded_disconnect(generation));
        };
        self.event_bus.publish(AppEvent::Lv1 {
            generation: transaction.cleared_generation,
            event: Lv1Event::Disconnected {
                reason: "Disconnected by user".to_string(),
            },
        });
        self.event_bus
            .publish_runtime_generation_changed(transaction.active_generation);
        tracing::info!(event = "lv1_disconnected", "Disconnected from LV1");
        Ok(ShowCommandResult { changed: true })
    }

    pub async fn current_runtime_generation(&self) -> RuntimeGeneration {
        self.inner.lock().await.generation.clone()
    }

    pub fn runtime_snapshot_source(&self) -> RuntimeSnapshotSource {
        RuntimeSnapshotSource {
            inner: self.inner.clone(),
            generation: self.show_peers.runtime_generation(),
        }
    }

    #[cfg(any(test, debug_assertions))]
    pub async fn current_lv1(&self) -> Option<Lv1ActorHandle> {
        self.inner.lock().await.handles.lv1.clone()
    }

    pub async fn current_fade(&self) -> Option<FadeEngineHandle> {
        self.inner.lock().await.handles.fade.clone()
    }

    pub fn scenes_handle(&self) -> ScenesHandle {
        self.scenes.clone()
    }

    pub fn cue_lists_handle(&self) -> CueListsHandle {
        self.cue_lists.clone()
    }

    async fn last_connected_lv1_identity(
        &self,
    ) -> Result<Option<crate::connection_state::Lv1SystemIdentity>, String> {
        let (reply, rx) = oneshot::channel();
        self.settings
            .send(SettingsCommand::GetLastConnectedLv1 { reply })
            .await
            .map_err(|_| "Settings are unavailable".to_string())?;
        rx.await
            .map_err(|_| "Settings reply channel is closed".to_string())
    }

    async fn remember_last_connected_lv1(
        &self,
        expected_generation: u64,
        identity: crate::connection_state::Lv1SystemIdentity,
    ) -> Result<(), String> {
        let (reply, rx) = oneshot::channel();
        let runtime_generation = self.current_runtime_generation().await;
        self.settings
            .send(SettingsCommand::SetLastConnectedLv1 {
                identity,
                runtime_generation,
                expected_generation,
                reply,
            })
            .await
            .map_err(|_| "Settings are unavailable".to_string())?;
        rx.await
            .map_err(|_| "Settings reply channel is closed".to_string())?
    }

    async fn log_last_connected_lv1_save_failure(&self, generation: u64, error: String) {
        let runtime_generation = self.current_runtime_generation().await;
        let _ = runtime_generation
            .if_current(generation, || {
                tracing::error!(
                    event = "last_connected_lv1_save_failed",
                    error = %error,
                    "Connected to LV1, but the connection could not be remembered for next startup"
                );
            })
            .await;
    }

    pub async fn connect_lv1_system<R: Runtime>(
        &self,
        app: AppHandle<R>,
        identity: crate::connection_state::Lv1SystemIdentity,
    ) -> Result<ConnectCommandResult, String> {
        self.abort_current_runtime().await;
        let generation = self
            .begin_connecting()
            .await
            .ok_or_else(|| "Failed to begin LV1 connection".to_string())?;
        self.connect_to_identity(app, generation, identity).await
    }

    pub async fn refresh_lv1_discovery(
        &self,
        timeout_ms: Option<u64>,
    ) -> Result<ShowCommandResult, String> {
        self.refresh_lv1_discovery_with(timeout_ms, crate::lv1::discover)
            .await
    }

    async fn refresh_lv1_discovery_with(
        &self,
        timeout_ms: Option<u64>,
        discover: impl FnOnce(
            crate::lv1::DiscoverOptions,
        ) -> std::io::Result<Vec<crate::lv1::DiscoveryEntry>>
        + Send
        + 'static,
    ) -> Result<ShowCommandResult, String> {
        // Serialize discovery results without holding up connection transitions or Show commands.
        let _discovery = self.discovery_lock.lock().await;
        let options = crate::lv1::DiscoverOptions {
            timeout: std::time::Duration::from_millis(timeout_ms.unwrap_or(1000).clamp(100, 6000)),
            ..Default::default()
        };
        let systems = tokio::task::spawn_blocking(move || discover(options))
            .await
            .map_err(|error| format!("LV1 discovery worker failed: {error}"))?
            .map_err(|error| format!("Failed to discover LV1 systems: {error}"))?
            .iter()
            .filter_map(crate::connection_state::system_from_discovery)
            .collect();
        let (reply, response) = oneshot::channel();
        self.show
            .send(ShowCommand::SetDiscoveredLv1Systems {
                systems,
                reply: Some(reply),
            })
            .await
            .map_err(|_| "Show state is unavailable".to_string())?;
        response
            .await
            .map_err(|_| "Show state reply channel is closed".to_string())
    }

    pub async fn startup_auto_connect_lv1<R: Runtime>(
        &self,
        app: AppHandle<R>,
    ) -> Result<ConnectCommandResult, String> {
        let Some(remembered) = self.last_connected_lv1_identity().await? else {
            return Ok(ConnectCommandResult { changed: false });
        };

        self.refresh_lv1_discovery(None).await?;

        let (reply, rx) = oneshot::channel();
        self.show
            .send(ShowCommand::InitialProjectionState { reply })
            .await
            .map_err(|_| "Show state is unavailable".to_string())?;
        let state = rx
            .await
            .map_err(|_| "Show state reply channel is closed".to_string())?;

        self.startup_auto_connect_with_discovered(app, remembered, &state.discovered_lv1_systems)
            .await
    }

    async fn startup_auto_connect_with_discovered<R: Runtime>(
        &self,
        app: AppHandle<R>,
        remembered: crate::connection_state::Lv1SystemIdentity,
        systems: &[crate::connection_state::DiscoveredLv1System],
    ) -> Result<ConnectCommandResult, String> {
        let Some(identity) =
            crate::connection_state::startup_auto_connect_target(&remembered, systems)
        else {
            tracing::debug!(
                event = "startup_auto_connect_no_match",
                "No safe discovered LV1 match for the remembered startup target"
            );
            return Ok(ConnectCommandResult { changed: false });
        };
        self.abort_current_runtime().await;
        let generation = self
            .begin_connecting()
            .await
            .ok_or_else(|| "Failed to begin LV1 startup auto-connect".to_string())?;
        self.connect_to_identity(app, generation, identity).await
    }

    pub async fn frontend_ready<R: Runtime>(
        &self,
        app: AppHandle<R>,
        logs: tokio::sync::broadcast::Receiver<UiLogEvent>,
    ) -> Result<(), String> {
        let (reply, rx) = oneshot::channel();
        self.show
            .send(ShowCommand::InitialProjectionState { reply })
            .await
            .map_err(|_| "Show state is unavailable".to_string())?;
        let initial_show_state = rx
            .await
            .map_err(|_| "Show state reply channel is closed".to_string())?;
        let initial_scenes_state = if let Some(scenes_handle) = self.show_peers.scenes() {
            let (reply, rx) = oneshot::channel();
            scenes_handle
                .send(crate::scenes::ScenesCommand::InitialProjectionState { reply })
                .await
                .map_err(|_| "Scenes state is unavailable".to_string())?;
            rx.await
                .map_err(|_| "Scenes state reply channel is closed".to_string())?
        } else {
            crate::scenes::ScenesProjectionState {
                scene_configs: Vec::new(),
                selected_scene_internal_id: None,
                scene_settings_clipboard_available: false,
                ready_generation: None,
            }
        };
        let initial_cue_lists_state = if let Some(cue_lists_handle) = self.show_peers.cue_lists() {
            let (reply, rx) = oneshot::channel();
            cue_lists_handle
                .send(crate::cue_lists::CueListsCommand::InitialProjectionState { reply })
                .await
                .map_err(|_| "Cue lists state is unavailable".to_string())?;
            rx.await
                .map_err(|_| "Cue lists state reply channel is closed".to_string())?
        } else {
            crate::cue_lists::CueListsProjectionState {
                document: crate::cue_lists::CueListDocument::default(),
                last_recall_status: None,
            }
        };
        let initial_settings = self.settings_snapshot().await?;
        let mut inner = self.inner.lock().await;
        if inner.frontend_ready {
            return Ok(());
        }
        inner.frontend_ready = true;
        let generation = inner.generation.current().await;
        inner.projector = Some(crate::projector::spawn_projector(
            crate::projector::ProjectorInputs {
                app,
                generation,
                initial_show_state,
                initial_scenes_state,
                initial_cue_lists_state,
                initial_settings,
                runtime_source: self.runtime_snapshot_source(),
                show: self.show.clone(),
                scenes: self.scenes.clone(),
                cue_lists: self.cue_lists.clone(),
                settings: self.settings.clone(),
                events: self.event_bus.subscribe(),
                logs,
            },
        ));
        Ok(())
    }
}

impl Default for AppLifecycle {
    fn default() -> Self {
        let event_bus = AppEventBus::default();
        let (show, show_task, show_peers, lockout) =
            crate::show::build_show_actor(event_bus.clone());
        let (settings, settings_task, initial_settings) =
            crate::settings::build_settings_actor(std::env::temp_dir(), event_bus.clone());
        show_task.spawn();
        settings_task.spawn();
        Self::new(
            event_bus,
            show,
            show_peers,
            lockout,
            settings,
            initial_settings,
        )
    }
}

fn log_lv1_connected(identity: &crate::connection_state::Lv1SystemIdentity) {
    let host = identity
        .host
        .as_deref()
        .unwrap_or(identity.address.as_str());
    tracing::info!(
        event = "lv1_connected",
        host = %host,
        port = identity.port,
        "LV1 connected"
    );
}

fn log_lv1_connect_requested(identity: &crate::connection_state::Lv1SystemIdentity) {
    tracing::debug!(
        event = "lv1_connect_requested",
        host = %identity.address,
        port = identity.port,
        "LV1 connect requested"
    );
}

fn log_lv1_connecting(identity: &crate::connection_state::Lv1SystemIdentity) {
    tracing::info!(
        event = "lv1_connecting",
        host = %identity.address,
        port = identity.port,
        "Connecting to LV1"
    );
}

fn log_lv1_connect_failed(identity: &crate::connection_state::Lv1SystemIdentity) {
    tracing::warn!(
        event = "lv1_connect_failed",
        host = %identity.address,
        port = identity.port,
        error = "LV1 did not connect",
        "LV1 did not connect"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection_state::{DiscoveredLv1Status, DiscoveredLv1System, Lv1SystemIdentity};
    use crate::cue_lists::CueListsEvent;
    use crate::fade::FadeEngineHandle;
    use crate::lv1::{Lv1Command, Lv1StateSnapshot, test_actor_handle};
    use crate::runtime::events::RuntimeLifecycleEvent;
    use crate::scenes::ScenesCommand;
    use crate::show::{ShowEvent, ShowProjectionReason};
    use std::path::PathBuf;
    use tauri::test::mock_app;
    use tokio::sync::{mpsc, oneshot};

    fn fake_lv1_handle(snapshot: Lv1StateSnapshot) -> crate::lv1::Lv1ActorHandle {
        let (tx, mut rx) = mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if let Lv1Command::GetState { reply } = command {
                    let _ = reply.send(snapshot.clone());
                }
            }
        });
        test_actor_handle(tx)
    }

    struct TestSettingsDir {
        path: PathBuf,
    }

    impl TestSettingsDir {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("asc-lifecycle-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).expect("temporary settings directory should exist");
            Self { path }
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TestSettingsDir {
        fn drop(&mut self) {
            if self.path.is_dir() {
                let _ = std::fs::remove_dir_all(&self.path);
            } else {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }

    struct LifecycleTestFixture {
        lifecycle: AppLifecycle,
        settings_dir: TestSettingsDir,
    }

    impl std::ops::Deref for LifecycleTestFixture {
        type Target = AppLifecycle;

        fn deref(&self) -> &Self::Target {
            &self.lifecycle
        }
    }

    fn settings_handle_for_test(
        settings_dir: &TestSettingsDir,
        event_bus: AppEventBus,
    ) -> SettingsHandle {
        let (settings, settings_task, _initial_settings) =
            crate::settings::build_settings_actor(settings_dir.path().to_path_buf(), event_bus);
        settings_task.spawn();
        settings
    }

    fn lifecycle_for_test_with_settings(
        event_bus: AppEventBus,
        settings: SettingsHandle,
    ) -> AppLifecycle {
        let (show, show_task, show_peers, lockout) =
            crate::show::build_show_actor(event_bus.clone());
        show_task.spawn();
        AppLifecycle::new(
            event_bus,
            show,
            show_peers,
            lockout,
            settings,
            crate::settings::AppSettings::default(),
        )
    }

    fn lifecycle_for_test(event_bus: AppEventBus) -> LifecycleTestFixture {
        let settings_dir = TestSettingsDir::new();
        let settings = settings_handle_for_test(&settings_dir, event_bus.clone());
        LifecycleTestFixture {
            lifecycle: lifecycle_for_test_with_settings(event_bus, settings),
            settings_dir,
        }
    }

    async fn started_runtime_for_test(
        lifecycle: &AppLifecycle,
        generation: u64,
        runtime_generation: RuntimeGeneration,
        event_bus: AppEventBus,
        lv1: Lv1ActorHandle,
        fade: FadeEngineHandle,
        before_connection_metadata: Option<BeforeConnectionMetadataHook>,
    ) -> StartedConnectedRuntime {
        assert!(
            lifecycle
                .install_runtime_transaction(
                    generation,
                    RuntimeHandles::with_runtime_targets(lv1.clone(), fade.clone()),
                )
                .await
                .is_ok(),
            "test runtime targets should install"
        );
        let initial_settings = lifecycle.settings_snapshot().await.unwrap();
        let _ = (runtime_generation, event_bus, initial_settings);
        StartedConnectedRuntime {
            lv1,
            fade,
            before_connection_metadata,
        }
    }

    async fn install_newer_runtime_with_identity(
        lifecycle: &AppLifecycle,
        identity: Lv1SystemIdentity,
    ) -> u64 {
        let generation = lifecycle.begin_connecting().await.unwrap();
        let (lv1_tx, _lv1_rx) = mpsc::channel(1);
        assert!(
            lifecycle
                .install_runtime_transaction(
                    generation,
                    RuntimeHandles::with_runtime_targets(
                        test_actor_handle(lv1_tx),
                        mpsc::channel(1).0,
                    ),
                )
                .await
                .is_ok()
        );
        let (reply, rx) = oneshot::channel();
        lifecycle
            .show
            .send(ShowCommand::CompleteLv1Connection {
                identity,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert!(rx.await.unwrap().accepted);
        generation
    }

    async fn set_last_connected_lv1(settings: &SettingsHandle, identity: Lv1SystemIdentity) {
        let (reply, rx) = oneshot::channel();
        settings
            .send(SettingsCommand::SetLastConnectedLv1 {
                identity,
                runtime_generation: RuntimeGeneration::default(),
                expected_generation: 0,
                reply,
            })
            .await
            .expect("remembered identity command should send");
        rx.await
            .expect("remembered identity reply should arrive")
            .expect("remembered identity should save");
    }

    async fn get_last_connected_lv1(settings: &SettingsHandle) -> Option<Lv1SystemIdentity> {
        let (reply, rx) = oneshot::channel();
        settings
            .send(SettingsCommand::GetLastConnectedLv1 { reply })
            .await
            .expect("remembered identity query should send");
        rx.await.expect("remembered identity reply should arrive")
    }

    fn connected_snapshot() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![],
            channels: vec![],
            ping_sequence: 0,
        }
    }

    fn disconnected_snapshot() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Disconnected,
            scene: None,
            scene_list: vec![],
            channels: vec![],
            ping_sequence: 0,
        }
    }

    fn identity(uuid: Option<&str>, host: Option<&str>, address: &str) -> Lv1SystemIdentity {
        Lv1SystemIdentity {
            uuid: uuid.map(str::to_string),
            host: host.map(str::to_string),
            address: address.to_string(),
            port: 50000,
        }
    }

    fn system(
        uuid: Option<&str>,
        host: Option<&str>,
        address: &str,
        status: DiscoveredLv1Status,
    ) -> DiscoveredLv1System {
        DiscoveredLv1System {
            identity: identity(uuid, host, address),
            status,
        }
    }

    #[tokio::test]
    async fn discovery_does_not_block_lockout_or_generation_changes() {
        let fixture = lifecycle_for_test(AppEventBus::default());
        let lifecycle = fixture.lifecycle.clone();
        let (entered, started) = oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let discovery = tokio::spawn(async move {
            lifecycle
                .refresh_lv1_discovery_with(Some(100), move |options| {
                    assert_eq!(options.timeout, std::time::Duration::from_millis(100));
                    entered.send(()).unwrap();
                    blocked
                        .recv_timeout(std::time::Duration::from_secs(1))
                        .map_err(std::io::Error::other)?;
                    Ok(Vec::new())
                })
                .await
        });
        started.await.unwrap();
        let generation = fixture.begin_connecting().await.unwrap();
        assert_eq!(generation, 1);
        let (reply, response) = oneshot::channel();
        fixture
            .show
            .send(ShowCommand::SetLockout {
                enabled: true,
                reply: Some(reply),
            })
            .await
            .unwrap();
        assert!(response.await.unwrap().changed);
        assert!(
            !discovery.is_finished(),
            "discovery must still be waiting for external I/O"
        );
        release.send(()).unwrap();
        assert!(!discovery.await.unwrap().unwrap().changed);
    }

    #[tokio::test]
    async fn discovery_failure_preserves_last_results_and_allows_retry() {
        let fixture = lifecycle_for_test(AppEventBus::default());
        let original = system(
            Some("console"),
            Some("FOH"),
            "192.0.2.10",
            DiscoveredLv1Status::Available,
        );
        let (reply, response) = oneshot::channel();
        fixture
            .show
            .send(ShowCommand::SetDiscoveredLv1Systems {
                systems: vec![original.clone()],
                reply: Some(reply),
            })
            .await
            .unwrap();
        response.await.unwrap();

        let error = fixture
            .refresh_lv1_discovery_with(None, |_| Err(std::io::Error::other("network unavailable")))
            .await
            .unwrap_err();
        assert_eq!(error, "Failed to discover LV1 systems: network unavailable");
        let (reply, response) = oneshot::channel();
        fixture
            .show
            .send(ShowCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(
            response.await.unwrap().discovered_lv1_systems,
            vec![original]
        );

        assert!(
            fixture
                .refresh_lv1_discovery_with(None, |_| Ok(Vec::new()))
                .await
                .unwrap()
                .changed
        );
    }

    #[test]
    fn lifecycle_test_fixture_removes_settings_directory() {
        let fixture = lifecycle_for_test(AppEventBus::default());
        let path = fixture.settings_dir.path().to_path_buf();

        assert!(path.exists());
        drop(fixture);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn frontend_ready_starts_projection_before_connected_scenes_exist() {
        let app = mock_app();
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus);
        let (log_tx, log_rx) = tokio::sync::broadcast::channel(8);

        let result = lifecycle.frontend_ready(app.handle().clone(), log_rx).await;

        assert!(result.is_ok());
        assert!(lifecycle.inner.lock().await.frontend_ready);
        drop(log_tx);
    }

    #[tokio::test]
    async fn lifecycle_allocates_monotonic_generations() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus);

        let first = lifecycle.begin_connecting().await.unwrap();
        lifecycle.abort_current_runtime().await;
        let second = lifecycle.begin_connecting().await.unwrap();

        assert!(second > first);
    }

    #[tokio::test]
    async fn building_runtime_does_not_install_cue_list_peer_before_acceptance() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus.clone());
        let identity = Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            address: "127.0.0.1".parse().unwrap(),
            host: Some("localhost".to_string()),
            port: 9000,
        };
        let runtime_generation = lifecycle.current_runtime_generation().await;

        let generation = lifecycle.begin_connecting().await.unwrap();
        let _built_runtime =
            build_connected_runtime(generation, runtime_generation, &identity, event_bus);

        assert!(lifecycle.show_peers.scenes().is_some());
        assert!(lifecycle.cue_lists_peers.scenes().is_some());
    }

    #[tokio::test]
    async fn accepted_connected_runtime_installs_scene_peers() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus.clone());
        let identity = Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            address: "127.0.0.1".parse().unwrap(),
            host: Some("localhost".to_string()),
            port: 9000,
        };

        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = fade_tx;
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            lv1,
            fade,
            None,
        )
        .await;

        let connect_result = lifecycle
            .finish_connect_transaction(identity, generation, started_runtime)
            .await;

        assert!(connect_result.is_ok());
        assert!(lifecycle.show_peers.scenes().is_some());
        assert!(lifecycle.cue_lists_peers.scenes().is_some());
        let (reply, response) = tokio::sync::oneshot::channel();
        lifecycle
            .scenes
            .send(crate::scenes::ScenesCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(response.await.unwrap().ready_generation, Some(generation));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn generation_flip_before_scene_peer_install_leaves_newer_peers_intact() {
        let capture = crate::test_support::TracingCapture::new();
        let _tracing_guard = capture.install();
        let event_bus = AppEventBus::default();
        let settings_dir = TestSettingsDir::new();
        let settings = settings_handle_for_test(&settings_dir, event_bus.clone());
        let lifecycle = lifecycle_for_test_with_settings(event_bus.clone(), settings.clone());
        let remembered = identity(Some("uuid-old"), Some("LV1-FOH"), "192.168.1.35");
        set_last_connected_lv1(&settings, remembered.clone()).await;
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let newer_generation = generation + 1;
        let (newer_scenes, newer_task, _newer_peers) = build_scenes_actor(
            newer_generation,
            runtime_generation.clone(),
            event_bus.clone(),
            event_bus.subscribe(),
            lifecycle.settings.clone(),
            lifecycle.settings_snapshot().await.unwrap(),
            lifecycle.lockout.clone(),
        );
        newer_task.spawn();
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = fade_tx;
        let (flip_tx, flip_rx) = oneshot::channel();
        let lifecycle_for_hook = lifecycle.clone();
        let newer_scenes_for_hook = newer_scenes.clone();
        let hook: Option<BeforeConnectionMetadataHook> =
            Some(Box::new(move |_runtime_generation: RuntimeGeneration| {
                Box::pin(async move {
                    let newer_generation = lifecycle_for_hook.begin_connecting().await.unwrap();
                    let (lv1_tx, _lv1_rx) = mpsc::channel(1);
                    let (fade_tx, _fade_rx) = mpsc::channel(1);
                    assert!(
                        lifecycle_for_hook
                            .install_runtime_transaction(
                                newer_generation,
                                RuntimeHandles::with_runtime_targets(
                                    test_actor_handle(lv1_tx),
                                    fade_tx,
                                ),
                            )
                            .await
                            .is_ok()
                    );
                    assert!(
                        lifecycle_for_hook
                            .install_accepted_scene_recall_fader(
                                newer_generation,
                                newer_scenes_for_hook,
                            )
                            .await
                    );
                    let (reply, rx) = oneshot::channel();
                    lifecycle_for_hook
                        .show
                        .send(ShowCommand::CompleteLv1Connection {
                            identity: identity(
                                Some("uuid-current"),
                                Some("LV1-FOH"),
                                "192.168.1.37",
                            ),
                            reply: Some(reply),
                        })
                        .await
                        .unwrap();
                    assert!(rx.await.unwrap().accepted);
                    let _ = flip_tx.send(());
                })
            }));
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            lv1,
            fade,
            hook,
        )
        .await;

        let result = lifecycle
            .finish_connect_transaction(
                identity(Some("uuid-new"), Some("LV1-FOH"), "192.168.1.36"),
                generation,
                started_runtime,
            )
            .await;

        assert!(flip_rx.await.is_ok());
        assert!(matches!(
            result,
            Err(message) if message == "generation is stale" || message == "LV1 connection was superseded"
        ));
        let show_scenes = lifecycle
            .show_peers
            .scenes()
            .expect("newer Show scenes peer should remain installed");
        let (show_reply, show_rx) = oneshot::channel();
        show_scenes
            .send(ScenesCommand::InitialProjectionState { reply: show_reply })
            .await
            .expect("newer Show scenes peer should accept mailbox commands");
        show_rx
            .await
            .expect("newer Show scenes peer should reply to mailbox commands");
        let cue_lists_scenes = lifecycle
            .cue_lists_peers
            .scenes()
            .expect("newer cue-list scenes peer should remain installed");
        let (cue_lists_reply, cue_lists_rx) = oneshot::channel();
        cue_lists_scenes
            .send(ScenesCommand::InitialProjectionState {
                reply: cue_lists_reply,
            })
            .await
            .expect("newer cue-list scenes peer should accept mailbox commands");
        cue_lists_rx
            .await
            .expect("newer cue-list scenes peer should reply to mailbox commands");
        let (reply, rx) = oneshot::channel();
        lifecycle
            .show
            .send(ShowCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(
            rx.await
                .unwrap()
                .connected_lv1_identity
                .unwrap()
                .uuid
                .as_deref(),
            Some("uuid-current")
        );
        assert_eq!(get_last_connected_lv1(&settings).await, Some(remembered));
        assert!(
            capture
                .matching("lv1_connected", tracing::Level::INFO)
                .is_empty()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn generation_flip_after_show_reply_suppresses_connected_success() {
        let capture = crate::test_support::TracingCapture::new();
        let _tracing_guard = capture.install();
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus.clone());
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = mpsc::channel(1);
        let lifecycle_for_hook = lifecycle.clone();
        lifecycle
            .set_before_connection_success(Box::new(move || {
                Box::pin(async move {
                    lifecycle_for_hook.begin_connecting().await;
                })
            }))
            .await;
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            lv1,
            fade_tx,
            None,
        )
        .await;

        let result = lifecycle
            .finish_connect_transaction(
                identity(Some("uuid-stale"), Some("LV1-FOH"), "192.168.1.36"),
                generation,
                started_runtime,
            )
            .await;

        assert!(matches!(result, Err(message) if message == "LV1 connection was superseded"));
        assert!(
            capture
                .matching("lv1_connected", tracing::Level::INFO)
                .is_empty()
        );
    }

    #[tokio::test]
    async fn lifecycle_publishes_active_generation_when_connecting_begins() {
        let event_bus = AppEventBus::default();
        let mut rx = event_bus.subscribe();
        let lifecycle = lifecycle_for_test(event_bus);

        let generation = lifecycle.begin_connecting().await.unwrap();

        let event = rx.recv().await.unwrap();
        assert!(matches!(
            event,
            AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation: event_generation })
                if event_generation == generation
        ));
    }

    #[tokio::test]
    async fn complete_connection_metadata_is_applied_atomically() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus);
        let mut events = lifecycle.event_bus.subscribe();
        let identity = Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };

        let generation = lifecycle.begin_connecting().await.unwrap();
        while events.try_recv().is_ok() {}
        lifecycle
            .complete_lv1_connection_metadata(generation, identity.clone())
            .await
            .expect("connected metadata should apply");

        loop {
            if matches!(
                events.recv().await.unwrap(),
                AppEvent::Show(ShowEvent::StateChanged {
                    reason: ShowProjectionReason::ConnectionMetadata,
                    ..
                })
            ) {
                break;
            }
        }

        let (reply, rx) = oneshot::channel();
        lifecycle
            .show
            .send(ShowCommand::InitialProjectionState { reply })
            .await
            .expect("connection metadata state should be requested");
        let state = rx.await.expect("connection metadata state should arrive");
        assert_eq!(state.connected_lv1_identity, Some(identity));
    }

    #[tokio::test]
    async fn connect_failure_clears_runtime_targets_after_failed_initial_lv1_state() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus);
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let lv1 = fake_lv1_handle(disconnected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = fade_tx;
        let identity = Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            lifecycle.event_bus.clone(),
            lv1,
            fade,
            None,
        )
        .await;

        let result = lifecycle
            .finish_connect_transaction(identity, generation, started_runtime)
            .await;

        assert!(matches!(result, Err(message) if message == "LV1 did not connect"));
        assert!(lifecycle.current_lv1().await.is_none());
        assert_eq!(
            lifecycle.inner.lock().await.runtime_handles_generation,
            None
        );
    }

    #[tokio::test]
    async fn generation_flip_before_failure_metadata_preserves_newer_show_identity() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus);
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let lv1 = fake_lv1_handle(disconnected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let current_identity = identity(Some("uuid-current"), Some("LV1-FOH"), "192.168.1.37");
        let lifecycle_for_hook = lifecycle.clone();
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            lifecycle.event_bus.clone(),
            lv1,
            fade_tx,
            Some(Box::new(move |_runtime_generation: RuntimeGeneration| {
                Box::pin(async move {
                    lifecycle_for_hook.begin_connecting().await;
                    let (reply, rx) = oneshot::channel();
                    lifecycle_for_hook
                        .show
                        .send(ShowCommand::CompleteLv1Connection {
                            identity: current_identity,
                            reply: Some(reply),
                        })
                        .await
                        .unwrap();
                    assert!(rx.await.unwrap().accepted);
                })
            })),
        )
        .await;

        let result = lifecycle
            .finish_connect_transaction(
                identity(Some("uuid-stale"), Some("LV1-FOH"), "192.168.1.36"),
                generation,
                started_runtime,
            )
            .await;

        assert!(matches!(result, Err(message) if message == "LV1 did not connect"));
        let (reply, rx) = oneshot::channel();
        lifecycle
            .show
            .send(ShowCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(
            rx.await
                .unwrap()
                .connected_lv1_identity
                .unwrap()
                .uuid
                .as_deref(),
            Some("uuid-current")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn accepted_failure_cleanup_survives_outer_cancellation() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let lifecycle = lifecycle_for_test(event_bus.clone());
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        while events.try_recv().is_ok() {}
        let (reached_tx, reached_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            fake_lv1_handle(disconnected_snapshot()),
            mpsc::channel(1).0,
            Some(Box::new(move |_| {
                Box::pin(async move {
                    reached_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                })
            })),
        )
        .await;
        let lifecycle_for_task = lifecycle.clone();
        let outer = tokio::spawn(async move {
            lifecycle_for_task
                .finish_connect_transaction(
                    identity(Some("uuid-failed"), Some("LV1-FOH"), "192.168.1.35"),
                    generation,
                    started_runtime,
                )
                .await
        });

        reached_rx.await.unwrap();
        outer.abort();
        release_tx.send(()).unwrap();
        loop {
            if matches!(events.recv().await.unwrap(), AppEvent::Runtime(
                RuntimeLifecycleEvent::ActiveGenerationChanged { generation: event_generation }
            ) if event_generation == generation + 1)
            {
                break;
            }
        }
        assert!(lifecycle.current_lv1().await.is_none());
        assert!(!lifecycle.scenes_handle().is_closed());
    }

    #[tokio::test]
    async fn initial_get_state_send_failure_cleans_up_without_clearing_newer_runtime() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus.clone());
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let (old_lv1_tx, old_lv1_rx) = mpsc::channel(1);
        drop(old_lv1_rx);
        let lifecycle_for_hook = lifecycle.clone();
        let newer_identity = identity(Some("new"), Some("LV1-FOH"), "192.0.2.21");
        let newer_identity_for_hook = newer_identity.clone();
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            test_actor_handle(old_lv1_tx),
            mpsc::channel(1).0,
            Some(Box::new(move |_| {
                let lifecycle = lifecycle_for_hook.clone();
                let identity = newer_identity_for_hook.clone();
                Box::pin(async move {
                    install_newer_runtime_with_identity(&lifecycle, identity).await;
                })
            })),
        )
        .await;

        let result = lifecycle
            .spawn_finish_connect_transaction(
                identity(Some("old"), Some("LV1-FOH"), "192.0.2.20"),
                generation,
                started_runtime,
            )
            .await
            .expect("detached finalizer should return a result");

        assert!(matches!(
            result,
            Err(error) if error.contains("Failed to request initial LV1 state")
        ));
        assert_eq!(lifecycle.active_generation().await, generation + 1);
        let (reply, rx) = oneshot::channel();
        lifecycle
            .show
            .send(ShowCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(
            rx.await.unwrap().connected_lv1_identity,
            Some(newer_identity)
        );
    }

    #[tokio::test]
    async fn initial_get_state_reply_closure_cleans_up_without_clearing_newer_runtime() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus.clone());
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let (old_lv1_tx, mut old_lv1_rx) = mpsc::channel(1);
        tokio::spawn(async move {
            if let Some(Lv1Command::GetState { reply }) = old_lv1_rx.recv().await {
                drop(reply);
            }
        });
        let lifecycle_for_hook = lifecycle.clone();
        let newer_identity = identity(Some("new"), Some("LV1-FOH"), "192.0.2.22");
        let newer_identity_for_hook = newer_identity.clone();
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            test_actor_handle(old_lv1_tx),
            mpsc::channel(1).0,
            Some(Box::new(move |_| {
                let lifecycle = lifecycle_for_hook.clone();
                let identity = newer_identity_for_hook.clone();
                Box::pin(async move {
                    install_newer_runtime_with_identity(&lifecycle, identity).await;
                })
            })),
        )
        .await;

        let result = lifecycle
            .spawn_finish_connect_transaction(
                identity(Some("old"), Some("LV1-FOH"), "192.0.2.20"),
                generation,
                started_runtime,
            )
            .await
            .expect("detached finalizer should return a result");

        assert!(matches!(
            result,
            Err(error) if error == AppCommandError::ReplyChannelClosed.to_string()
        ));
        assert_eq!(lifecycle.active_generation().await, generation + 1);
        let (reply, rx) = oneshot::channel();
        lifecycle
            .show
            .send(ShowCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(
            rx.await.unwrap().connected_lv1_identity,
            Some(newer_identity)
        );
    }

    #[tokio::test]
    async fn cancelled_connect_future_finalizes_after_pending_get_state() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus.clone());
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let (old_lv1_tx, mut old_lv1_rx) = mpsc::channel(1);
        let old_lv1 = test_actor_handle(old_lv1_tx);
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            old_lv1,
            mpsc::channel(1).0,
            None,
        )
        .await;

        let result_rx = lifecycle.spawn_finish_connect_transaction(
            identity(Some("old"), Some("LV1-FOH"), "192.0.2.10"),
            generation,
            started_runtime,
        );
        let outer = tokio::spawn(result_rx);
        let Lv1Command::GetState {
            reply: old_state_reply,
        } = old_lv1_rx.recv().await.unwrap()
        else {
            panic!("expected pending GetState");
        };
        outer.abort();

        let newer_generation = lifecycle.begin_connecting().await.unwrap();
        let newer_identity = identity(Some("new"), Some("LV1-FOH"), "192.0.2.11");
        let (newer_lv1_tx, _newer_lv1_rx) = mpsc::channel(1);
        assert!(
            lifecycle
                .install_runtime_transaction(
                    newer_generation,
                    RuntimeHandles::with_runtime_targets(
                        test_actor_handle(newer_lv1_tx),
                        mpsc::channel(1).0,
                    ),
                )
                .await
                .is_ok(),
            "newer runtime should install"
        );
        let (metadata_reply, metadata_rx) = oneshot::channel();
        lifecycle
            .show
            .send(ShowCommand::CompleteLv1Connection {
                identity: newer_identity.clone(),
                reply: Some(metadata_reply),
            })
            .await
            .unwrap();
        assert!(metadata_rx.await.unwrap().accepted);

        old_state_reply
            .send(disconnected_snapshot())
            .expect("pending GetState should be released");
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                let (reply, rx) = oneshot::channel();
                lifecycle
                    .show
                    .send(ShowCommand::InitialProjectionState { reply })
                    .await
                    .unwrap();
                if rx.await.unwrap().connected_lv1_identity == Some(newer_identity.clone()) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancelled connect finalizer should finish without clearing newer identity");
        assert_eq!(lifecycle.active_generation().await, newer_generation);
    }

    #[tokio::test]
    async fn connect_installs_runtime_targets_before_connection_metadata() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus.clone());
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let lv1 = fake_lv1_handle(connected_snapshot());
        let lv1_for_assertion = lv1.clone();
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = fade_tx;
        let identity = Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50000,
        };
        let (seen_tx, seen_rx) = oneshot::channel();
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            lv1,
            fade,
            Some(Box::new(move |_runtime_generation: RuntimeGeneration| {
                Box::pin(async move {
                    let (reply, rx) = oneshot::channel();
                    let ok = lv1_for_assertion
                        .send(Lv1Command::GetState { reply })
                        .await
                        .is_ok()
                        && matches!(rx.await, Ok(snapshot) if snapshot.connection == ConnectionStatus::Connected);
                    let _ = seen_tx.send(ok);
                })
            })),
        )
        .await;

        let result = lifecycle
            .finish_connect_transaction(identity, generation, started_runtime)
            .await;

        assert!(result.is_ok());
        assert!(matches!(seen_rx.await, Ok(true)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn connect_completion_logs_lv1_connected_for_ui_log_projection() {
        let capture = crate::test_support::TracingCapture::new();
        let _tracing_guard = capture.install();
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus.clone());
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = fade_tx;
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            lv1,
            fade,
            None,
        )
        .await;

        let result = lifecycle
            .finish_connect_transaction(
                Lv1SystemIdentity {
                    uuid: Some("uuid-1".to_string()),
                    host: Some("LV1-FOH".to_string()),
                    address: "192.168.1.35".to_string(),
                    port: 50000,
                },
                generation,
                started_runtime,
            )
            .await;

        assert!(result.is_ok());
        assert!(
            capture
                .matching("lv1_connected", tracing::Level::INFO)
                .iter()
                .any(|log| {
                    log.fields.get("host").map(String::as_str) == Some("LV1-FOH")
                        && log.fields.get("port").map(String::as_str) == Some("50000")
                })
        );
    }

    #[tokio::test]
    async fn connect_retains_scene_recall_fader_handle() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus.clone());
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = fade_tx;
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            lv1,
            fade,
            None,
        )
        .await;

        let result = lifecycle
            .finish_connect_transaction(
                Lv1SystemIdentity {
                    uuid: Some("uuid-1".to_string()),
                    host: Some("LV1-FOH".to_string()),
                    address: "192.168.1.35".to_string(),
                    port: 50000,
                },
                generation,
                started_runtime,
            )
            .await;

        assert!(result.is_ok());
        assert!(!lifecycle.scenes_handle().is_closed());
    }

    #[tokio::test]
    async fn app_lifecycle_installs_cue_lists_handle_for_commands() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus);

        let _command_handle = lifecycle.cue_lists_handle();
        assert!(lifecycle.show_peers.cue_lists().is_some());
    }

    #[tokio::test]
    async fn app_lifecycle_cue_list_command_mutates_and_publishes_projection_without_lv1() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let lifecycle = lifecycle_for_test(event_bus);

        let command_handle = lifecycle.cue_lists_handle();
        let (reply, rx) = oneshot::channel();
        command_handle
            .send(crate::cue_lists::CueListsCommand::CreateCueList {
                name: "Smoke Cue List".to_string(),
                reply: Some(reply),
            })
            .await
            .expect("create cue list command should send");
        let created = rx
            .await
            .expect("create cue list reply should arrive")
            .expect("create cue list should succeed")
            .cue_list
            .expect("create cue list should return cue list");

        loop {
            if let AppEvent::CueLists(CueListsEvent::StateChanged { state, .. }) =
                events.recv().await.unwrap()
            {
                assert_eq!(state.document.active_cue_list_id, Some(created.id));
                assert!(
                    state
                        .document
                        .cue_lists
                        .iter()
                        .any(|list| { list.id == created.id && list.name == "Smoke Cue List" })
                );
                break;
            }
        }

        command_handle
            .send(crate::cue_lists::CueListsCommand::Shutdown)
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn connect_lv1_system_attempts_selected_identity() {
        let capture = crate::test_support::TracingCapture::new();
        let _tracing_guard = capture.install();
        let app = mock_app();
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus);

        let result = lifecycle
            .connect_lv1_system(
                app.handle().clone(),
                Lv1SystemIdentity {
                    uuid: None,
                    host: Some("Unreachable".to_string()),
                    address: "127.0.0.1".to_string(),
                    port: 1,
                },
            )
            .await;

        assert!(
            result.is_err(),
            "unreachable selected identity should fail instead of returning a false success"
        );
        assert!(
            !capture
                .matching("lv1_connect_requested", tracing::Level::DEBUG)
                .is_empty()
        );
        assert!(
            !capture
                .matching("lv1_connecting", tracing::Level::INFO)
                .is_empty()
        );
        assert!(
            !capture
                .matching("lv1_connect_failed", tracing::Level::WARN)
                .is_empty()
        );
    }

    #[tokio::test]
    async fn startup_without_remembered_identity_does_not_advance_generation() {
        let app = mock_app();
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus);
        let before = lifecycle.active_generation().await;

        let result = lifecycle
            .startup_auto_connect_lv1(app.handle().clone())
            .await
            .expect("startup without a stored identity should not fail");

        assert!(!result.changed);
        assert_eq!(lifecycle.active_generation().await, before);
    }

    #[tokio::test]
    async fn ambiguous_startup_match_preserves_generation_and_remembered_identity() {
        let app = mock_app();
        let event_bus = AppEventBus::default();
        let settings_dir = TestSettingsDir::new();
        let settings = settings_handle_for_test(&settings_dir, event_bus.clone());
        let lifecycle = lifecycle_for_test_with_settings(event_bus, settings.clone());
        let remembered = identity(None, Some("LV1-FOH"), "192.168.1.35");
        set_last_connected_lv1(&settings, remembered.clone()).await;
        let systems = vec![
            system(
                None,
                Some("LV1-FOH"),
                "10.0.0.20",
                DiscoveredLv1Status::Available,
            ),
            system(
                None,
                Some("LV1-FOH"),
                "10.0.0.21",
                DiscoveredLv1Status::Available,
            ),
        ];
        let before = lifecycle.active_generation().await;

        let result = lifecycle
            .startup_auto_connect_with_discovered(
                app.handle().clone(),
                remembered.clone(),
                &systems,
            )
            .await
            .expect("ambiguous startup match should not fail");

        assert!(!result.changed);
        assert_eq!(lifecycle.active_generation().await, before);
        assert_eq!(get_last_connected_lv1(&settings).await, Some(remembered));
    }

    #[tokio::test]
    async fn accepted_connect_remembers_confirmed_identity() {
        let event_bus = AppEventBus::default();
        let settings_dir = TestSettingsDir::new();
        let settings = settings_handle_for_test(&settings_dir, event_bus.clone());
        let lifecycle = lifecycle_for_test_with_settings(event_bus.clone(), settings.clone());
        let identity = identity(Some("uuid-1"), Some("LV1-FOH"), "192.168.1.35");
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = fade_tx;
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            lv1,
            fade,
            None,
        )
        .await;

        let result = lifecycle
            .finish_connect_transaction(identity.clone(), generation, started_runtime)
            .await;

        assert!(result.is_ok());
        assert_eq!(get_last_connected_lv1(&settings).await, Some(identity));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn accepted_connect_logs_one_error_when_identity_cannot_be_remembered() {
        let capture = crate::test_support::TracingCapture::new();
        let _tracing_guard = capture.install();
        let event_bus = AppEventBus::default();
        let settings_dir = TestSettingsDir::new();
        std::fs::remove_dir_all(settings_dir.path())
            .expect("settings test directory should remove");
        std::fs::write(settings_dir.path(), "not a directory")
            .expect("settings test path should become a file");
        let settings = settings_handle_for_test(&settings_dir, event_bus.clone());
        let lifecycle = lifecycle_for_test_with_settings(event_bus.clone(), settings);
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = fade_tx;
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            lv1,
            fade,
            None,
        )
        .await;

        let result = lifecycle
            .finish_connect_transaction(
                identity(Some("uuid-1"), Some("LV1-FOH"), "192.168.1.35"),
                generation,
                started_runtime,
            )
            .await;

        assert!(result.is_ok());
        assert!(lifecycle.current_lv1().await.is_some());
        let errors = capture.matching("last_connected_lv1_save_failed", tracing::Level::ERROR);
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].message.as_deref(),
            Some("Connected to LV1, but the connection could not be remembered for next startup")
        );
    }

    #[tokio::test]
    async fn stale_connect_does_not_replace_remembered_identity() {
        let event_bus = AppEventBus::default();
        let settings_dir = TestSettingsDir::new();
        let settings = settings_handle_for_test(&settings_dir, event_bus.clone());
        let lifecycle = lifecycle_for_test_with_settings(event_bus.clone(), settings.clone());
        let remembered = identity(Some("uuid-old"), Some("LV1-FOH"), "192.168.1.35");
        set_last_connected_lv1(&settings, remembered.clone()).await;
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = fade_tx;
        let lifecycle_for_hook = lifecycle.clone();
        let (flip_tx, flip_rx) = oneshot::channel();
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            lv1,
            fade,
            Some(Box::new(move |_runtime_generation: RuntimeGeneration| {
                Box::pin(async move {
                    lifecycle_for_hook.begin_connecting().await;
                    let _ = flip_tx.send(());
                })
            })),
        )
        .await;

        let result = lifecycle
            .finish_connect_transaction(
                identity(Some("uuid-new"), Some("LV1-FOH"), "192.168.1.36"),
                generation,
                started_runtime,
            )
            .await;

        assert!(flip_rx.await.is_ok());
        assert!(matches!(
            result,
            Err(message) if message == "generation is stale" || message == "LV1 connection was superseded"
        ));
        assert_eq!(get_last_connected_lv1(&settings).await, Some(remembered));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stale_persistence_handoff_does_not_store_identity_or_log_an_error() {
        let capture = crate::test_support::TracingCapture::new();
        let _tracing_guard = capture.install();
        let event_bus = AppEventBus::default();
        let settings_dir = TestSettingsDir::new();
        let (settings, settings_task, _) = crate::settings::build_settings_actor(
            settings_dir.path().to_path_buf(),
            event_bus.clone(),
        );
        let (write_received_tx, write_received_rx) = oneshot::channel();
        let (release_write_tx, release_write_rx) = oneshot::channel();
        settings_task
            .pause_set_last_connected_lv1(write_received_tx, release_write_rx)
            .spawn();
        let lifecycle = lifecycle_for_test_with_settings(event_bus.clone(), settings.clone());
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = fade_tx;
        let identity = identity(Some("uuid-new"), Some("LV1-FOH"), "192.168.1.36");
        let lifecycle_for_connect = lifecycle.clone();
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            lv1,
            fade,
            None,
        )
        .await;

        let connect = tokio::spawn(async move {
            lifecycle_for_connect
                .finish_connect_transaction(identity, generation, started_runtime)
                .await
        });

        write_received_rx
            .await
            .expect("settings write should be reached after scene-peer acceptance");
        assert!(!lifecycle.scenes_handle().is_closed());
        lifecycle.begin_connecting().await.unwrap();
        std::fs::remove_dir_all(settings_dir.path())
            .expect("settings directory should be removable while the write is paused");
        std::fs::write(settings_dir.path(), "not a directory")
            .expect("settings path should prevent a stale write");
        release_write_tx.send(()).unwrap();

        assert!(
            matches!(connect.await.unwrap(), Err(message) if message == "LV1 connection was superseded")
        );
        assert_eq!(get_last_connected_lv1(&settings).await, None);
        assert!(
            capture
                .matching("last_connected_lv1_save_failed", tracing::Level::ERROR)
                .is_empty()
        );
    }

    #[tokio::test]
    async fn stale_runtime_install_returns_abortable_handles() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus);
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = fade_tx;
        let handles = RuntimeHandles::with_runtime_targets(lv1, fade);

        let rejection = lifecycle
            .install_runtime_transaction(1, handles)
            .await
            .expect_err("stale generation should reject the runtime install");

        let mut handles = rejection.into_handles();
        handles.abort_all();
        assert_eq!(
            lifecycle.inner.lock().await.runtime_handles_generation,
            None
        );
    }

    #[tokio::test]
    async fn accepted_runtime_install_disconnects_observably() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let lifecycle = lifecycle_for_test(event_bus);
        let generation = lifecycle.begin_connecting().await.unwrap();
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);

        let install = lifecycle
            .install_runtime_transaction(
                generation,
                RuntimeHandles::with_runtime_targets(lv1, fade_tx),
            )
            .await;
        assert!(install.is_ok());

        while events.try_recv().is_ok() {}
        let result = lifecycle.disconnect_current_runtime().await.unwrap();

        assert!(result.changed);
        assert!(lifecycle.current_lv1().await.is_none());
        assert_eq!(lifecycle.active_generation().await, generation + 1);
        loop {
            if matches!(
                events.recv().await.unwrap(),
                AppEvent::Lv1 {
                    generation: event_generation,
                    event: Lv1Event::Disconnected { .. },
                } if event_generation == generation
            ) {
                break;
            }
        }
        assert!(matches!(
            events.recv().await.unwrap(),
            AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged {
                generation: event_generation,
            }) if event_generation == generation + 1
        ));
    }

    #[tokio::test]
    async fn rejected_connection_cleanup_preserves_newer_scene_peers() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus.clone());
        let rejected_generation = lifecycle.begin_connecting().await.unwrap();
        let accepted_generation = lifecycle.begin_connecting().await.unwrap();
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);

        let install = lifecycle
            .install_runtime_transaction(
                accepted_generation,
                RuntimeHandles::with_runtime_targets(lv1, fade_tx),
            )
            .await;
        assert!(install.is_ok());
        let (scenes, _scenes_task, _scenes_peers) = build_scenes_actor(
            accepted_generation,
            lifecycle.current_runtime_generation().await,
            event_bus.clone(),
            event_bus.subscribe(),
            lifecycle.settings.clone(),
            lifecycle.settings_snapshot().await.unwrap(),
            lifecycle.lockout.clone(),
        );
        assert!(
            lifecycle
                .install_accepted_scene_recall_fader(accepted_generation, scenes)
                .await
        );

        lifecycle
            .abort_rejected_connection_transaction(rejected_generation, RuntimeHandles::default())
            .await;

        assert_eq!(
            lifecycle.inner.lock().await.runtime_handles_generation,
            Some(accepted_generation)
        );
        assert!(lifecycle.current_lv1().await.is_some());
        assert!(lifecycle.show_peers.scenes().is_some());
        assert!(lifecycle.cue_lists_peers.scenes().is_some());
    }

    #[tokio::test]
    async fn stale_runtime_build_does_not_replace_newer_show_lv1_peer() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus.clone());
        let stale_generation = lifecycle.begin_connecting().await.unwrap();
        let accepted_generation = lifecycle.begin_connecting().await.unwrap();
        let (newer_tx, mut newer_rx) = mpsc::channel(1);
        let newer_lv1 = crate::lv1::test_actor_handle(newer_tx);
        let (fade_tx, _fade_rx) = mpsc::channel(1);
        let install = lifecycle
            .install_runtime_transaction(
                accepted_generation,
                RuntimeHandles::with_runtime_targets(newer_lv1, fade_tx),
            )
            .await;
        assert!(install.is_ok());

        let stale_runtime = build_connected_runtime(
            stale_generation,
            lifecycle.current_runtime_generation().await,
            &identity(Some("uuid-stale"), Some("LV1-Stale"), "192.168.1.36"),
            event_bus.clone(),
        );
        let Err(rejection) = lifecycle
            .install_runtime_transaction(stale_generation, stale_runtime.runtime_targets())
            .await
        else {
            panic!("stale runtime install should be rejected");
        };
        lifecycle
            .abort_rejected_connection_transaction(stale_generation, rejection.into_handles())
            .await;
        drop(stale_runtime);

        let (reply, result) = oneshot::channel();
        lifecycle
            .show
            .send(ShowCommand::NewShowFileFromCurrentLv1 { reply: Some(reply) })
            .await
            .expect("show command should send");
        let command = tokio::time::timeout(std::time::Duration::from_millis(100), newer_rx.recv())
            .await
            .expect("newer Show LV1 peer should receive GetState")
            .expect("newer LV1 receiver should remain connected");
        let Lv1Command::GetState { reply } = command else {
            panic!("Show should request the newer LV1 state");
        };
        reply
            .send(connected_snapshot())
            .expect("Show LV1 state reply should send");
        let Lv1Command::GetState { reply } =
            tokio::time::timeout(std::time::Duration::from_millis(100), newer_rx.recv())
                .await
                .expect("Show should revalidate newer LV1 state")
                .expect("newer LV1 receiver should remain connected")
        else {
            panic!("Show should revalidate with GetState");
        };
        reply
            .send(connected_snapshot())
            .expect("Show LV1 revalidation reply should send");
        assert!(result.await.unwrap().is_ok());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn disconnect_public_path_installed_newer_runtime_is_unchanged() {
        let capture = crate::test_support::TracingCapture::new();
        let _tracing_guard = capture.install();
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let lifecycle = lifecycle_for_test(event_bus);

        let original_generation = lifecycle.begin_connecting().await.unwrap();
        let (old_lv1_tx, _old_lv1_rx) = mpsc::channel(1);
        let (old_fade_tx, _old_fade_rx) = mpsc::channel(1);
        assert!(
            lifecycle
                .install_runtime_transaction(
                    original_generation,
                    RuntimeHandles::with_runtime_targets(
                        test_actor_handle(old_lv1_tx),
                        old_fade_tx,
                    ),
                )
                .await
                .is_ok()
        );
        while events.try_recv().is_ok() {}

        let (cleanup_reached_tx, cleanup_reached_rx) = oneshot::channel();
        let (resume_cleanup_tx, resume_cleanup_rx) = oneshot::channel();
        lifecycle
            .set_before_disconnect_cleanup(Box::new(move || {
                Box::pin(async move {
                    cleanup_reached_tx.send(()).unwrap();
                    resume_cleanup_rx.await.unwrap();
                })
            }))
            .await;

        let disconnect_lifecycle = lifecycle.lifecycle.clone();
        let disconnect = tokio::spawn(async move {
            disconnect_lifecycle
                .disconnect_current_runtime()
                .await
                .unwrap()
        });
        cleanup_reached_rx.await.unwrap();

        let newer_generation = lifecycle.begin_connecting().await.unwrap();
        let (newer_lv1_tx, mut newer_lv1_rx) = mpsc::channel(1);
        let (newer_fade_tx, mut newer_fade_rx) = mpsc::channel(1);
        assert!(
            lifecycle
                .install_runtime_transaction(
                    newer_generation,
                    RuntimeHandles::with_runtime_targets(
                        test_actor_handle(newer_lv1_tx),
                        newer_fade_tx,
                    ),
                )
                .await
                .is_ok()
        );
        while events.try_recv().is_ok() {}

        resume_cleanup_tx.send(()).unwrap();
        assert!(!disconnect.await.unwrap().changed);
        lifecycle
            .current_lv1()
            .await
            .unwrap()
            .send(Lv1Command::GetState {
                reply: oneshot::channel().0,
            })
            .await
            .unwrap();
        assert!(matches!(
            newer_lv1_rx.recv().await,
            Some(Lv1Command::GetState { .. })
        ));
        lifecycle
            .current_fade()
            .await
            .unwrap()
            .send(crate::fade::FadeCommand::AbortAll { reply: None })
            .await
            .unwrap();
        assert!(matches!(
            newer_fade_rx.recv().await,
            Some(crate::fade::FadeCommand::AbortAll { .. })
        ));
        while let Ok(event) = events.try_recv() {
            assert!(matches!(event, AppEvent::Scenes { .. }));
        }
        assert!(
            capture
                .matching("lv1_disconnected", tracing::Level::INFO)
                .is_empty()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn two_same_generation_disconnects_have_one_success_sequence() {
        let capture = crate::test_support::TracingCapture::new();
        let _tracing_guard = capture.install();
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let lifecycle = lifecycle_for_test(event_bus);
        let generation = lifecycle.begin_connecting().await.unwrap();
        let (lv1_tx, _lv1_rx) = mpsc::channel(1);
        let (fade_tx, _fade_rx) = mpsc::channel(1);
        assert!(
            lifecycle
                .install_runtime_transaction(
                    generation,
                    RuntimeHandles::with_runtime_targets(test_actor_handle(lv1_tx), fade_tx,),
                )
                .await
                .is_ok()
        );
        while events.try_recv().is_ok() {}

        let (cleanup_reached_tx, cleanup_reached_rx) = oneshot::channel();
        let (resume_cleanup_tx, resume_cleanup_rx) = oneshot::channel();
        lifecycle
            .set_before_disconnect_cleanup(Box::new(move || {
                Box::pin(async move {
                    cleanup_reached_tx.send(()).unwrap();
                    resume_cleanup_rx.await.unwrap();
                })
            }))
            .await;
        let first_lifecycle = lifecycle.lifecycle.clone();
        let first =
            tokio::spawn(
                async move { first_lifecycle.disconnect_current_runtime().await.unwrap() },
            );
        cleanup_reached_rx.await.unwrap();
        let second = lifecycle.disconnect_current_runtime().await.unwrap();
        resume_cleanup_tx.send(()).unwrap();
        let first = first.await.unwrap();

        assert_eq!(usize::from(first.changed) + usize::from(second.changed), 1);
        let published = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
        assert_eq!(
            published
                .iter()
                .filter(|event| matches!(
                    event,
                    AppEvent::Lv1 {
                        event: Lv1Event::Disconnected { .. },
                        ..
                    }
                ))
                .count(),
            1
        );
        assert_eq!(
            published
                .iter()
                .filter(|event| matches!(
                    event,
                    AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { .. })
                ))
                .count(),
            1
        );
        assert_eq!(
            capture
                .matching("lv1_disconnected", tracing::Level::INFO)
                .len(),
            1
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stale_generation_does_not_block_newer_disconnect() {
        let capture = crate::test_support::TracingCapture::new();
        let _tracing_guard = capture.install();
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let lifecycle = lifecycle_for_test(event_bus);

        let stale_generation = lifecycle.begin_connecting().await.unwrap();
        let newer_generation = lifecycle.begin_connecting().await.unwrap();
        while events.try_recv().is_ok() {}

        let (newer_lv1_tx, mut newer_lv1_rx) = mpsc::channel(8);
        let newer_lv1 = test_actor_handle(newer_lv1_tx);
        let (newer_fade_tx, mut newer_fade_rx) = mpsc::channel(1);
        assert!(
            lifecycle
                .install_runtime_transaction(
                    newer_generation,
                    RuntimeHandles::with_runtime_targets(newer_lv1, newer_fade_tx,),
                )
                .await
                .is_ok(),
            "newer runtime should install"
        );
        let newer_identity = identity(Some("newer"), Some("LV1-FOH"), "192.0.2.20");
        lifecycle
            .show
            .send(ShowCommand::CompleteLv1Connection {
                identity: newer_identity.clone(),
                reply: None,
            })
            .await
            .expect("newer identity should be accepted");
        while matches!(events.try_recv(), Ok(AppEvent::Show(_))) {}

        let result = lifecycle
            .disconnect_runtime_generation(stale_generation)
            .await
            .expect("superseded disconnect should be a safe no-op");

        assert!(!result.changed);
        assert_eq!(lifecycle.active_generation().await, newer_generation);
        let (identity_reply, identity_rx) = oneshot::channel();
        lifecycle
            .show
            .send(ShowCommand::InitialProjectionState {
                reply: identity_reply,
            })
            .await
            .unwrap();
        assert_eq!(
            identity_rx.await.unwrap().connected_lv1_identity,
            Some(newer_identity)
        );
        let current_lv1 = lifecycle
            .current_lv1()
            .await
            .expect("newer LV1 handle should survive stale cleanup");
        let (state_reply, state_rx) = oneshot::channel();
        current_lv1
            .send(Lv1Command::GetState { reply: state_reply })
            .await
            .expect("newer LV1 mailbox should accept commands");
        let Lv1Command::GetState { reply } = newer_lv1_rx
            .recv()
            .await
            .expect("newer LV1 actor should receive GetState")
        else {
            panic!("expected GetState through newer LV1 mailbox");
        };
        reply
            .send(connected_snapshot())
            .expect("newer LV1 actor should reply");
        assert_eq!(
            state_rx
                .await
                .expect("newer LV1 reply should arrive")
                .connection,
            ConnectionStatus::Connected
        );

        lifecycle
            .current_fade()
            .await
            .expect("newer fade handle should survive stale cleanup")
            .send(crate::fade::FadeCommand::AbortAll { reply: None })
            .await
            .expect("newer fade mailbox should accept commands");
        assert!(matches!(
            newer_fade_rx.recv().await,
            Some(crate::fade::FadeCommand::AbortAll { reply: None })
        ));

        let (show_reply, show_rx) = oneshot::channel();
        lifecycle
            .show
            .send(ShowCommand::NewShowFileFromCurrentLv1 {
                reply: Some(show_reply),
            })
            .await
            .expect("Show mailbox should accept new-show command");
        let Lv1Command::GetState { reply } = newer_lv1_rx
            .recv()
            .await
            .expect("Show should use the newer LV1 peer")
        else {
            panic!("expected Show to request newer LV1 state");
        };
        reply
            .send(connected_snapshot())
            .expect("Show LV1 state reply should send");
        let Lv1Command::GetState { reply } =
            tokio::time::timeout(std::time::Duration::from_millis(100), newer_lv1_rx.recv())
                .await
                .expect("Show should revalidate newer LV1 state")
                .expect("newer LV1 receiver should remain connected")
        else {
            panic!("Show should revalidate with GetState");
        };
        reply
            .send(connected_snapshot())
            .expect("Show LV1 revalidation reply should send");
        show_rx
            .await
            .expect("Show reply should arrive")
            .expect("Show should create a session through the app-lifetime Scenes actor");

        let cue_lists = lifecycle.cue_lists_handle();
        let (create_reply, create_rx) = oneshot::channel();
        cue_lists
            .send(crate::cue_lists::CueListsCommand::CreateCueList {
                name: "Generation Safety".to_string(),
                reply: Some(create_reply),
            })
            .await
            .expect("Cue Lists mailbox should accept create command");
        create_rx
            .await
            .expect("create reply should arrive")
            .expect("cue list should be created");
        let scene_internal_id = uuid::Uuid::new_v4();
        let (add_reply, add_rx) = oneshot::channel();
        cue_lists
            .send(crate::cue_lists::CueListsCommand::AddSceneToActiveCueList {
                scene_internal_id,
                insert_index: 0,
                reply: Some(add_reply),
            })
            .await
            .expect("Cue Lists mailbox should accept add command");
        let entry = add_rx
            .await
            .expect("add reply should arrive")
            .expect("scene should be added to active cue list")
            .entry
            .expect("added cue entry should be returned");
        let (cue_reply, cue_rx) = oneshot::channel();
        cue_lists
            .send(crate::cue_lists::CueListsCommand::CueEntry {
                cue_entry_id: Some(entry.id),
                reply: Some(cue_reply),
            })
            .await
            .expect("Cue Lists mailbox should accept cue command");
        cue_rx
            .await
            .expect("cue reply should arrive")
            .expect("entry should be cued");
        let (recall_reply, recall_rx) = oneshot::channel();
        cue_lists
            .send(crate::cue_lists::CueListsCommand::RecallCuedCue {
                reply: recall_reply,
            })
            .await
            .expect("Cue Lists mailbox should accept recall command");
        let _ = scene_internal_id;
        assert!(matches!(
            recall_rx.await.expect("recall reply should arrive"),
            Err(crate::runtime::errors::AppCommandError::ScenesUnavailable)
        ));
        assert!(
            capture
                .matching("lv1_disconnected", tracing::Level::INFO)
                .is_empty()
        );
        assert!(
            !capture
                .matching("lv1_disconnect_superseded", tracing::Level::DEBUG)
                .is_empty()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn disconnect_current_runtime_publishes_active_generation_disconnect() {
        let capture = crate::test_support::TracingCapture::new();
        let _tracing_guard = capture.install();
        let event_bus = AppEventBus::default();
        let mut rx = event_bus.subscribe();
        let lifecycle = lifecycle_for_test(event_bus);

        let generation = lifecycle.begin_connecting().await.unwrap();
        assert!(matches!(
            rx.recv().await.unwrap(),
            AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation: event_generation })
                if event_generation == generation
        ));
        let connected_identity = identity(Some("disconnect-target"), Some("LV1-FOH"), "192.0.2.30");
        lifecycle
            .show
            .send(ShowCommand::CompleteLv1Connection {
                identity: connected_identity,
                reply: None,
            })
            .await
            .unwrap();
        loop {
            if matches!(
                rx.recv().await.unwrap(),
                AppEvent::Show(ShowEvent::StateChanged { .. })
            ) {
                break;
            }
        }
        let result = lifecycle.disconnect_current_runtime().await.unwrap();

        assert!(result.changed);
        loop {
            if matches!(
                rx.recv().await.unwrap(),
                AppEvent::Lv1 { generation: event_generation, event: Lv1Event::Disconnected { .. } }
                    if event_generation == generation
            ) {
                break;
            }
        }
        assert!(matches!(
            rx.recv().await.unwrap(),
            AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation: event_generation })
                if event_generation == generation + 1
        ));
        assert!(
            !capture
                .matching("lv1_disconnect_requested", tracing::Level::DEBUG)
                .is_empty()
        );
        assert!(
            !capture
                .matching("lv1_disconnected", tracing::Level::INFO)
                .is_empty()
        );
        let (reply, state_rx) = oneshot::channel();
        lifecycle
            .show
            .send(ShowCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(state_rx.await.unwrap().connected_lv1_identity, None);
    }
}

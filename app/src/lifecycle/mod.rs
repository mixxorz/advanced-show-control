//! App runtime lifecycle ownership.

use std::sync::Arc;

#[cfg(test)]
use std::future::Future;
#[cfg(test)]
use std::pin::Pin;
use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;
use tracing::instrument::WithSubscriber;

use crate::cue_lists::CueListsHandle;
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

/// @cc [owner:mixxorz,label:architecture;safety] complete-generation-runtime
/// An installed runtime MUST bind one generation to both its LV1 and Fade endpoints as one value;
/// lifecycle state MUST NOT install, clear, or retag either endpoint independently.
#[derive(Clone)]
struct InstalledRuntime {
    generation: u64,
    lv1: Lv1ActorHandle,
    fade: FadeEngineHandle,
}

#[cfg(test)]
type BeforeConnectionMetadataHook =
    Box<dyn FnOnce(RuntimeGeneration) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

#[cfg(test)]
type DisconnectTestHook = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

struct BuiltConnectedRuntime {
    runtime: InstalledRuntime,
    lv1_task: crate::lv1::Lv1ActorTask,
    fade_task: crate::fade::FadeEngineTask,
}

impl BuiltConnectedRuntime {
    fn spawn_lv1_and_fade(self) -> StartedConnectedRuntime {
        self.lv1_task.spawn();
        self.fade_task.spawn();
        StartedConnectedRuntime {
            runtime: self.runtime,
            #[cfg(test)]
            before_connection_metadata: None,
        }
    }
}

struct StartedConnectedRuntime {
    runtime: InstalledRuntime,
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
        runtime: InstalledRuntime {
            generation,
            lv1,
            fade,
        },
        lv1_task,
        fade_task,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeClearTransaction {
    cleared_generation: u64,
    active_generation: u64,
}

struct LifecycleInner {
    generation: RuntimeGeneration,
    connecting: bool,
    runtime: Option<InstalledRuntime>,
    projection_sink: Option<crate::projector::ProjectionSink>,
    projector: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub struct RuntimeSnapshotSource {
    inner: Arc<Mutex<LifecycleInner>>,
    generation: RuntimeGeneration,
}

impl RuntimeSnapshotSource {
    /// @cc [owner:mixxorz,label:safety] current-runtime-snapshot-only
    /// A snapshot MUST return an LV1 handle only when an installed runtime exists and its generation
    /// equals the shared active generation read while lifecycle state is locked; otherwise it MUST
    /// return `None` rather than expose a stale endpoint.
    pub async fn connected_lv1(&self) -> Option<(u64, Lv1ActorHandle)> {
        let inner = self.inner.lock().await;
        let generation = inner.generation.current().await;
        let runtime = inner
            .runtime
            .as_ref()
            .filter(|runtime| runtime.generation == generation)?;
        Some((generation, runtime.lv1.clone()))
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
    /// @cc [owner:mixxorz,label:architecture] app-lifetime-document-owner
    /// Construction MUST create and start exactly one app-lifetime Scenes owner, derive Cue Lists
    /// from that same owner, and install the Scenes handle into Show; connection transitions MUST
    /// reuse these handles rather than replace their documents.
    pub fn new(
        event_bus: AppEventBus,
        show: ShowStateHandle,
        show_peers: ShowActorPeers,
        lockout: ShowLockoutReader,
        settings: SettingsHandle,
        initial_settings: crate::settings::AppSettings,
    ) -> Self {
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
        let cue_lists = scenes_task.cue_lists_handle();
        show_peers.set_scenes(scenes.clone());
        scenes_task.spawn();

        Self {
            inner: Arc::new(Mutex::new(LifecycleInner {
                generation: runtime_generation,
                connecting: false,
                runtime: None,
                projection_sink: None,
                projector: None,
            })),
            event_bus,
            show,
            show_peers,
            #[cfg(test)]
            lockout,
            cue_lists,
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

    #[cfg(test)]
    async fn settings_snapshot(&self) -> Result<crate::settings::AppSettings, String> {
        Ok(self.event_bus.state().borrow().settings.clone())
    }

    /// @cc [owner:mixxorz,label:safety;ordering] begin-connection-generation-first
    /// Beginning a connection MUST serialize with lifecycle transitions, advance the active
    /// generation before marking the connection pending, and publish that new generation before
    /// returning it.
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

    /// @cc [owner:mixxorz,label:safety] install-current-runtime-only
    /// Runtime installation MUST accept only an exact active-generation match. Acceptance MUST
    /// install Show's LV1 peer and the complete runtime before clearing `connecting`; rejection MUST
    /// return the candidate without changing installed runtime state or Show's peer.
    async fn install_runtime_transaction(
        &self,
        runtime: InstalledRuntime,
    ) -> Result<(), InstalledRuntime> {
        let mut inner = self.inner.lock().await;
        if inner.generation.current().await != runtime.generation {
            return Err(runtime);
        }

        self.show_peers
            .set_lv1(runtime.generation, runtime.lv1.clone());
        inner.runtime = Some(runtime);
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

    /// @cc [owner:mixxorz,label:safety] install-scene-peers-from-current-runtime
    /// Scene peers MUST be installed only from a complete installed runtime whose generation equals
    /// both `generation` and the active generation; every mismatch or absence MUST return `false`
    /// without altering peers.
    async fn install_accepted_scene_peers(&self, generation: u64) -> bool {
        let inner = self.inner.lock().await;
        if inner.generation.current().await != generation {
            return false;
        }
        let Some(runtime) = inner
            .runtime
            .as_ref()
            .filter(|runtime| runtime.generation == generation)
        else {
            return false;
        };
        self.scenes_peers.set_peers_for_generation(
            generation,
            runtime.lv1.clone(),
            runtime.fade.clone(),
        );
        true
    }

    /// @cc [owner:mixxorz,label:safety] clear-runtime-compare-and-advance
    /// Clearing MUST atomically require `expected_generation` to be active, advance the generation,
    /// remove the installed runtime, clear connecting state, and clear Show/Scenes peers for only
    /// the expected generation. A stale request MUST perform none of these effects.
    async fn clear_runtime_if_current(
        &self,
        expected_generation: u64,
    ) -> Option<RuntimeClearTransaction> {
        let mut inner = self.inner.lock().await;
        let active_generation = inner
            .generation
            .advance_if_current(expected_generation)
            .await?;

        inner.runtime = None;
        inner.connecting = false;
        self.show_peers.clear_lv1(expected_generation);
        self.scenes_peers
            .clear_peers_for_generation(expected_generation);

        Some(RuntimeClearTransaction {
            cleared_generation: expected_generation,
            active_generation,
        })
    }

    /// @cc [owner:mixxorz,label:safety;ordering] publish-generation-after-clear
    /// A clear transaction MUST publish the newly active generation only after current-generation
    /// runtime and peer cleanup succeeds; a stale clear request MUST publish nothing.
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

    /// @cc [owner:mixxorz,label:safety] rejected-cleanup-generation-scoped
    /// Cleanup of a rejected candidate MUST remove lifecycle and Scenes state only if it still
    /// belongs to the candidate generation, and MUST clear Show's peer through its generation-aware
    /// operation; it MUST NOT disturb a newer runtime or its peers.
    async fn abort_rejected_connection_transaction(&self, candidate: InstalledRuntime) {
        let generation = candidate.generation;
        drop(candidate);
        let mut inner = self.inner.lock().await;
        if inner.runtime.as_ref().map(|runtime| runtime.generation) == Some(generation) {
            inner.runtime = None;
            self.scenes_peers.clear_peers_for_generation(generation);
        }
        drop(inner);
        self.show_peers.clear_lv1(generation);
    }

    /// @cc [owner:mixxorz,label:safety;ordering] connect-install-before-start
    /// A connection candidate MUST be installed under its generation fence before its LV1/Fade
    /// tasks are started. A rejected candidate MUST be cleaned up and return a stale-generation
    /// error without starting those tasks.
    pub async fn connect_to_identity(
        &self,
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
        if let Err(runtime) = self
            .install_runtime_transaction(built_runtime.runtime.clone())
            .await
        {
            self.abort_rejected_connection_transaction(runtime).await;
            return Err("generation is stale".to_string());
        }
        let started_runtime = built_runtime.spawn_lv1_and_fade();

        let result = self.spawn_finish_connect_transaction(identity, started_runtime);
        result
            .await
            .map_err(|_| "LV1 connection finalizer task was cancelled".to_string())?
    }

    /// @cc [owner:mixxorz,label:reliability] detached-connect-finalization
    /// Connection finalization MUST run in its own task so cancellation of the requesting future
    /// cannot strand an installed candidate or prevent its generation-fenced success/failure
    /// cleanup; task cancellation MUST surface as an error to a receiver that remains.
    fn spawn_finish_connect_transaction(
        &self,
        identity: crate::connection_state::Lv1SystemIdentity,
        started_runtime: StartedConnectedRuntime,
    ) -> oneshot::Receiver<Result<ConnectCommandResult, String>> {
        let (reply, result) = oneshot::channel();
        let lifecycle = self.clone();
        let subscriber = tracing::dispatcher::get_default(|dispatcher| dispatcher.clone());
        tokio::spawn(
            async move {
                let outcome = lifecycle
                    .finish_connect_transaction(identity, started_runtime)
                    .await;
                let _ = reply.send(outcome);
            }
            .with_subscriber(subscriber),
        );
        result
    }

    /// @cc [owner:mixxorz,label:safety] connected-snapshot-required
    /// Connection completion MUST request an initial LV1 snapshot and MUST accept the candidate only
    /// when that snapshot reports `Connected`; command-send failure, reply closure, or any other
    /// status MUST enter generation-fenced failure finalization and return an error.
    async fn finish_connect_transaction(
        &self,
        identity: crate::connection_state::Lv1SystemIdentity,
        started_runtime: StartedConnectedRuntime,
    ) -> Result<ConnectCommandResult, String> {
        let StartedConnectedRuntime {
            runtime,
            #[cfg(test)]
            before_connection_metadata,
        } = started_runtime;
        let generation = runtime.generation;

        let (reply, rx) = oneshot::channel();
        if let Err(error) = runtime.lv1.send(Lv1Command::GetState { reply }).await {
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
            let finalizer = tokio::spawn(
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
        let finalizer = tokio::spawn(
            async move {
                lifecycle
                    .finalize_connection_metadata(
                        identity,
                        runtime,
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
    /**
     * @cc [owner:mixxorz,label:safety;ordering] accepted-connect-readiness-order
     * A connected candidate MUST first have Show metadata accepted for its generation, then install
     * that generation's Scene peers, then deliver `RuntimePeersReady` with the confirmed initial
     * scene list. Any rejection or unavailable Scenes actor MUST clean up only the candidate and
     * return an error; connected success MUST remain generation-fenced after all awaits.
     */
    /**
     * @cc [owner:mixxorz,label:reliability] remembered-identity-best-effort
     * Failure to persist an otherwise accepted connected identity MUST NOT fail or tear down the
     * connection; it MUST emit the dedicated error only while that generation remains current.
     */
    async fn finalize_connection_metadata(
        &self,
        identity: crate::connection_state::Lv1SystemIdentity,
        runtime: InstalledRuntime,
        _initial_snapshot: crate::lv1::Lv1StateSnapshot,
        #[cfg(test)] before_connection_metadata: Option<BeforeConnectionMetadataHook>,
    ) -> Result<ConnectCommandResult, String> {
        let generation = runtime.generation;
        #[cfg(test)]
        if let Some(before_connection_metadata) = before_connection_metadata {
            before_connection_metadata(self.current_runtime_generation().await).await;
        }

        let completion = match self
            .set_lv1_connection_metadata(generation, Some(identity.clone()))
            .await
        {
            Ok(completion) => completion,
            Err(error) => {
                self.abort_rejected_connection_transaction(runtime).await;
                return Err(error.to_string());
            }
        };
        if !completion.accepted {
            self.abort_rejected_connection_transaction(runtime).await;
            return Err("LV1 connection was superseded".to_string());
        }

        if !self.install_accepted_scene_peers(generation).await {
            self.abort_rejected_connection_transaction(runtime).await;
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
            self.abort_rejected_connection_transaction(runtime).await;
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
        if let Some(accepted) = accepted {
            Ok(accepted)
        } else {
            self.abort_rejected_connection_transaction(runtime).await;
            Err("LV1 connection was superseded".to_string())
        }
    }

    /// @cc [owner:mixxorz,label:safety] failed-connect-generation-fenced-cleanup
    /// Connection failure MUST request metadata clearing for only the candidate generation, emit
    /// the failure log only when that clear was accepted and the generation is still current, and
    /// run generation-fenced runtime cleanup without overwriting newer connection state.
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
        let failure = self.set_lv1_connection_metadata(generation, None).await;
        if failure.as_ref().is_ok_and(|outcome| outcome.accepted) {
            let generation_guard = self.current_runtime_generation().await;
            let _ = generation_guard
                .if_current(generation, || log_lv1_connect_failed(&identity))
                .await;
        }
        self.clear_runtime_transaction(generation).await;
        Err(error)
    }

    /// @cc [owner:mixxorz,label:safety] show-controls-metadata-acceptance
    /// Connection identity changes MUST be delegated to Show with the expected generation and MUST
    /// surface mailbox or reply-channel failure; lifecycle MUST NOT infer acceptance or mutate Show
    /// metadata directly.
    async fn set_lv1_connection_metadata(
        &self,
        expected_generation: u64,
        identity: Option<crate::connection_state::Lv1SystemIdentity>,
    ) -> Result<crate::show::CompleteConnectionOutcome, AppCommandError> {
        let (reply, rx) = oneshot::channel();
        self.show
            .send(ShowCommand::SetLv1ConnectionIfCurrent {
                identity,
                expected_generation,
                reply,
            })
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

    /// @cc [owner:mixxorz,label:safety] generation-fenced-disconnect-effects
    /// For an `Ok` result targeting generation `N`, this function MUST clear connection metadata
    /// and runtime endpoints, publish its disconnect facts, emit the success log, and return
    /// `changed: true` only when generation checks accept `N` as current. If `N` was superseded, it
    /// MUST preserve the newer generation's state, publish no disconnect success facts or log, and
    /// return `changed: false`. Show mailbox or reply failures MUST return `Err` without clearing
    /// runtime endpoints or publishing disconnect success facts or log.
    async fn finish_disconnect(&self, generation: u64) -> Result<ShowCommandResult, String> {
        let cleared = self
            .set_lv1_connection_metadata(generation, None)
            .await
            .map_err(|error| error.to_string())?;
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
        self.inner
            .lock()
            .await
            .runtime
            .as_ref()
            .map(|runtime| runtime.lv1.clone())
    }

    pub async fn current_fade(&self) -> Option<FadeEngineHandle> {
        self.inner
            .lock()
            .await
            .runtime
            .as_ref()
            .map(|runtime| runtime.fade.clone())
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

    /// @cc [owner:mixxorz,label:safety;ordering] explicit-connect-replaces-runtime
    /// An explicit connect MUST invalidate and clear the current runtime before allocating the new
    /// connection generation, so old generation tasks cannot remain admitted during replacement.
    pub async fn connect_lv1_system(
        &self,
        identity: crate::connection_state::Lv1SystemIdentity,
    ) -> Result<ConnectCommandResult, String> {
        self.abort_current_runtime().await;
        let generation = self
            .begin_connecting()
            .await
            .ok_or_else(|| "Failed to begin LV1 connection".to_string())?;
        self.connect_to_identity(generation, identity).await
    }

    pub async fn refresh_lv1_discovery(
        &self,
        timeout_ms: Option<u64>,
    ) -> Result<ShowCommandResult, String> {
        self.refresh_lv1_discovery_with(timeout_ms, crate::lv1::discover)
            .await
    }

    /**
     * @cc [owner:mixxorz,label:concurrency] serialized-nonblocking-discovery
     * Discovery calls MUST be serialized so older results cannot overwrite newer results, while
     * blocking network discovery MUST run off the async worker and MUST NOT hold the lifecycle
     * transition lock or Show mailbox.
     */
    /**
     * @cc [owner:mixxorz,label:reliability] discovery-failure-preserves-results
     * Worker or discovery failure MUST return an error without sending replacement results to Show.
     * Show mailbox-send or reply-channel failure MUST return an error, although a closed reply can
     * occur after Show has accepted the replacement.
     */
    /**
     * @cc [owner:mixxorz,label:product] bounded-discovery-timeout
     * The effective discovery timeout MUST default to 1000 ms and clamp caller values to the
     * inclusive 100–6000 ms range before network I/O.
     */
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

    /// @cc [owner:mixxorz,label:product] no-remembered-startup-noop
    /// When no remembered LV1 identity exists, startup auto-connect MUST return `changed: false`
    /// without discovery, runtime teardown, or generation advancement; settings/discovery/Show
    /// failures after an identity is found MUST be returned.
    pub async fn startup_auto_connect_lv1(&self) -> Result<ConnectCommandResult, String> {
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

        self.startup_auto_connect_with_discovered(remembered, &state.discovered_lv1_systems)
            .await
    }

    /// @cc [owner:mixxorz,label:safety] startup-auto-connect-safe-match-only
    /// Startup auto-connect MUST preserve the active generation, current runtime, and remembered
    /// identity when discovery does not yield one safe target; only an unambiguous accepted target
    /// may trigger runtime abort and a new connection generation.
    async fn startup_auto_connect_with_discovered(
        &self,
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
        self.connect_to_identity(generation, identity).await
    }

    /// @cc [owner:mixxorz,label:architecture] projector-starts-once
    /// The first presentation-ready call MUST atomically mark readiness and start one projector from the
    /// current generation and retained/event/log sources; subsequent calls MUST succeed without
    /// replacing or starting another projector.
    pub async fn frontend_ready(
        &self,
        logs: tokio::sync::broadcast::Receiver<UiLogEvent>,
    ) -> Result<crate::projector::ProjectionSubscription, String> {
        let mut inner = self.inner.lock().await;
        if let Some(sink) = &inner.projection_sink {
            return Ok(sink.subscribe());
        }
        let generation = inner.generation.current().await;
        let (sink, subscription) = crate::projector::projection_channel();
        inner.projector = Some(crate::projector::spawn_projector(
            crate::projector::ProjectorInputs {
                sink: sink.clone(),
                generation,
                state: self.event_bus.state(),
                runtime_source: self.runtime_snapshot_source(),
                events: self.event_bus.subscribe(),
                logs,
            },
        ));
        inner.projection_sink = Some(sink);
        Ok(subscription)
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
    use crate::fade::FadeEngineHandle;
    use crate::lv1::{Lv1Command, Lv1StateSnapshot, test_actor_handle};
    use crate::runtime::events::RuntimeLifecycleEvent;
    use crate::scenes::ScenesCommand;

    use std::path::PathBuf;
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
        _settings_dir: TestSettingsDir,
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
            _settings_dir: settings_dir,
        }
    }

    fn lifecycle_for_test_with_show(
        event_bus: AppEventBus,
        show: ShowStateHandle,
    ) -> LifecycleTestFixture {
        let settings_dir = TestSettingsDir::new();
        let settings = settings_handle_for_test(&settings_dir, event_bus.clone());
        let (_unused_show, show_task, show_peers, lockout) =
            crate::show::build_show_actor(event_bus.clone());
        drop(show_task);
        LifecycleTestFixture {
            lifecycle: AppLifecycle::new(
                event_bus,
                show,
                show_peers,
                lockout,
                settings,
                crate::settings::AppSettings::default(),
            ),
            _settings_dir: settings_dir,
        }
    }

    fn installed_runtime(
        generation: u64,
        lv1: Lv1ActorHandle,
        fade: FadeEngineHandle,
    ) -> InstalledRuntime {
        InstalledRuntime {
            generation,
            lv1,
            fade,
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
                .install_runtime_transaction(installed_runtime(
                    generation,
                    lv1.clone(),
                    fade.clone(),
                ))
                .await
                .is_ok(),
            "test runtime targets should install"
        );
        let initial_settings = lifecycle.settings_snapshot().await.unwrap();
        let _ = (runtime_generation, event_bus, initial_settings);
        StartedConnectedRuntime {
            runtime: installed_runtime(generation, lv1, fade),
            before_connection_metadata,
        }
    }

    async fn set_show_connection(
        lifecycle: &AppLifecycle,
        generation: u64,
        identity: Lv1SystemIdentity,
    ) -> crate::show::CompleteConnectionOutcome {
        let (reply, rx) = oneshot::channel();
        lifecycle
            .show
            .send(ShowCommand::SetLv1ConnectionIfCurrent {
                identity: Some(identity),
                expected_generation: generation,
                reply,
            })
            .await
            .expect("connection metadata command should send");
        rx.await.expect("connection metadata reply should arrive")
    }

    async fn install_newer_runtime_with_identity(
        lifecycle: &AppLifecycle,
        identity: Lv1SystemIdentity,
    ) -> u64 {
        let generation = lifecycle.begin_connecting().await.unwrap();
        let (lv1_tx, _lv1_rx) = mpsc::channel(1);
        assert!(
            lifecycle
                .install_runtime_transaction(installed_runtime(
                    generation,
                    test_actor_handle(lv1_tx),
                    mpsc::channel(1).0,
                ),)
                .await
                .is_ok()
        );
        assert!(
            set_show_connection(lifecycle, generation, identity)
                .await
                .accepted
        );
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
    async fn frontend_ready_starts_one_projector_and_returns_latest_snapshot_subscriptions() {
        let fixture = lifecycle_for_test(AppEventBus::default());
        let (_first_logs, first_log_rx) = tokio::sync::broadcast::channel(1);
        let (_second_logs, second_log_rx) = tokio::sync::broadcast::channel(1);

        let mut first = fixture.frontend_ready(first_log_rx).await.unwrap();
        let mut second = fixture.frontend_ready(second_log_rx).await.unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(1), first.changed())
            .await
            .expect("first subscription should receive the initial snapshot")
            .expect("projector should remain available");
        tokio::time::timeout(std::time::Duration::from_secs(1), second.changed())
            .await
            .expect("second subscription should receive the initial snapshot")
            .expect("projector should remain available");
        let first_snapshot = first.latest().unwrap();
        let second_snapshot = second.latest().unwrap();
        assert_eq!(first_snapshot.state_version, second_snapshot.state_version);
        assert_eq!(first_snapshot, second_snapshot);
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
            .finish_connect_transaction(identity, started_runtime)
            .await;

        assert!(connect_result.is_ok());
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
                            .install_runtime_transaction(installed_runtime(
                                newer_generation,
                                test_actor_handle(lv1_tx),
                                fade_tx,
                            ),)
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
                    assert!(
                        set_show_connection(
                            &lifecycle_for_hook,
                            newer_generation,
                            identity(Some("uuid-current"), Some("LV1-FOH"), "192.168.1.37",),
                        )
                        .await
                        .accepted
                    );
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
        let (cue_lists_reply, cue_lists_rx) = oneshot::channel();
        lifecycle
            .cue_lists_handle()
            .send(crate::cue_lists::CueListsCommand::InitialProjectionState {
                reply: cue_lists_reply,
            })
            .await
            .expect("cue-list owner should accept mailbox commands");
        cue_lists_rx
            .await
            .expect("cue-list owner should reply to mailbox commands");
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

    #[tokio::test(flavor = "current_thread")]
    async fn final_fence_supersession_cleans_candidate_before_newer_runtime_install() {
        let capture = crate::test_support::TracingCapture::new();
        let _tracing_guard = capture.install();
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let lifecycle = lifecycle_for_test(event_bus.clone());
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let (candidate_lv1_tx, mut candidate_lv1_rx) = mpsc::channel(8);
        let (candidate_lv1_closed_tx, candidate_lv1_closed_rx) = oneshot::channel();
        tokio::spawn(async move {
            while let Some(command) = candidate_lv1_rx.recv().await {
                if let Lv1Command::GetState { reply } = command {
                    let _ = reply.send(connected_snapshot());
                }
            }
            let _ = candidate_lv1_closed_tx.send(());
        });
        let (candidate_fade_tx, mut candidate_fade_rx) = mpsc::channel(1);
        let (newer_generation_tx, newer_generation_rx) = oneshot::channel();
        let lifecycle_for_hook = lifecycle.clone();
        lifecycle
            .set_before_connection_success(Box::new(move || {
                Box::pin(async move {
                    let newer_generation = lifecycle_for_hook.begin_connecting().await.unwrap();
                    newer_generation_tx.send(newer_generation).unwrap();
                })
            }))
            .await;
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            test_actor_handle(candidate_lv1_tx),
            candidate_fade_tx,
            None,
        )
        .await;
        while events.try_recv().is_ok() {}

        let result = lifecycle
            .finish_connect_transaction(
                identity(Some("candidate"), Some("LV1-FOH"), "192.0.2.40"),
                started_runtime,
            )
            .await;
        let newer_generation = newer_generation_rx.await.unwrap();

        assert!(matches!(result, Err(message) if message == "LV1 connection was superseded"));
        assert!(lifecycle.current_lv1().await.is_none());
        assert!(lifecycle.current_fade().await.is_none());
        tokio::time::timeout(std::time::Duration::from_secs(1), candidate_lv1_closed_rx)
            .await
            .expect("candidate LV1 task should stop after all endpoints are released")
            .expect("candidate LV1 task should report shutdown");
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), candidate_fade_rx.recv())
                .await
                .expect("candidate Fade endpoint should be released")
                .is_none()
        );

        let (newer_lv1_tx, mut newer_lv1_rx) = mpsc::channel(1);
        let (newer_fade_tx, mut newer_fade_rx) = mpsc::channel(1);
        assert!(
            lifecycle
                .install_runtime_transaction(installed_runtime(
                    newer_generation,
                    test_actor_handle(newer_lv1_tx),
                    newer_fade_tx,
                ))
                .await
                .is_ok()
        );
        assert!(
            lifecycle
                .install_accepted_scene_peers(newer_generation)
                .await
        );
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
        assert!(
            capture
                .matching("lv1_connected", tracing::Level::INFO)
                .is_empty()
        );
        while let Ok(event) = events.try_recv() {
            assert!(!matches!(
                event,
                AppEvent::Lv1 {
                    event: Lv1Event::Connected,
                    ..
                }
            ));
        }
    }

    async fn assert_closed_show_reply_cleans_candidate_without_touching_newer_runtime() {
        let capture = crate::test_support::TracingCapture::new();
        let _tracing_guard = capture.install();
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let (show, mut show_rx) = mpsc::channel(1);
        let (admitted_tx, admitted_rx) = oneshot::channel();
        tokio::spawn(async move {
            let Some(ShowCommand::SetLv1ConnectionIfCurrent { reply, .. }) = show_rx.recv().await
            else {
                panic!("expected connection metadata command");
            };
            admitted_tx.send(()).unwrap();
            drop(reply);
        });
        let lifecycle = lifecycle_for_test_with_show(event_bus.clone(), show);
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let (candidate_lv1_tx, mut candidate_lv1_rx) = mpsc::channel(8);
        let (candidate_lv1_closed_tx, candidate_lv1_closed_rx) = oneshot::channel();
        tokio::spawn(async move {
            while let Some(command) = candidate_lv1_rx.recv().await {
                if let Lv1Command::GetState { reply } = command {
                    let _ = reply.send(connected_snapshot());
                }
            }
            let _ = candidate_lv1_closed_tx.send(());
        });
        let (candidate_fade_tx, mut candidate_fade_rx) = mpsc::channel(1);
        let (newer_lv1_tx, mut newer_lv1_rx) = mpsc::channel(1);
        let (newer_fade_tx, mut newer_fade_rx) = mpsc::channel(1);
        let (newer_generation_tx, newer_generation_rx) = oneshot::channel();
        let lifecycle_for_hook = lifecycle.clone();
        let hook = Some(Box::new(move |_runtime_generation: RuntimeGeneration| {
            Box::pin(async move {
                let newer_generation = lifecycle_for_hook.begin_connecting().await.unwrap();
                assert!(
                    lifecycle_for_hook
                        .install_runtime_transaction(installed_runtime(
                            newer_generation,
                            test_actor_handle(newer_lv1_tx),
                            newer_fade_tx,
                        ))
                        .await
                        .is_ok()
                );
                assert!(
                    lifecycle_for_hook
                        .install_accepted_scene_peers(newer_generation)
                        .await
                );
                let (reply, response) = oneshot::channel();
                lifecycle_for_hook
                    .scenes
                    .send(ScenesCommand::RuntimePeersReady {
                        generation: newer_generation,
                        initial_scene_list: vec![],
                        reply,
                    })
                    .await
                    .unwrap();
                response.await.unwrap().unwrap();
                newer_generation_tx.send(newer_generation).unwrap();
            }) as Pin<Box<dyn Future<Output = ()> + Send>>
        }) as BeforeConnectionMetadataHook);
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            test_actor_handle(candidate_lv1_tx),
            candidate_fade_tx,
            hook,
        )
        .await;
        while events.try_recv().is_ok() {}

        let result = lifecycle
            .finish_connect_transaction(
                identity(Some("candidate"), Some("LV1-FOH"), "192.0.2.40"),
                started_runtime,
            )
            .await;
        admitted_rx.await.unwrap();
        assert_eq!(
            result.unwrap_err(),
            AppCommandError::ReplyChannelClosed.to_string()
        );
        let newer_generation = newer_generation_rx.await.unwrap();
        assert_eq!(lifecycle.active_generation().await, newer_generation);
        tokio::time::timeout(std::time::Duration::from_secs(1), candidate_lv1_closed_rx)
            .await
            .expect("candidate LV1 endpoint should be released")
            .expect("candidate LV1 task should report shutdown");
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), candidate_fade_rx.recv())
                .await
                .expect("candidate Fade endpoint should be released")
                .is_none()
        );

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
        let (reply, response) = oneshot::channel();
        lifecycle
            .scenes
            .send(ScenesCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(
            response.await.unwrap().ready_generation,
            Some(newer_generation)
        );
        assert!(
            capture
                .matching("lv1_connected", tracing::Level::INFO)
                .is_empty()
        );
        while let Ok(event) = events.try_recv() {
            assert!(!matches!(
                event,
                AppEvent::Lv1 {
                    event: Lv1Event::Connected,
                    ..
                }
            ));
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unavailable_show_mailbox_cleans_installed_candidate() {
        let capture = crate::test_support::TracingCapture::new();
        let _tracing_guard = capture.install();
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let (show, show_rx) = mpsc::channel(1);
        drop(show_rx);
        let lifecycle = lifecycle_for_test_with_show(event_bus.clone(), show);
        let generation = lifecycle.begin_connecting().await.unwrap();
        let runtime_generation = lifecycle.current_runtime_generation().await;
        let (candidate_lv1_tx, mut candidate_lv1_rx) = mpsc::channel(8);
        let (candidate_lv1_closed_tx, candidate_lv1_closed_rx) = oneshot::channel();
        tokio::spawn(async move {
            while let Some(command) = candidate_lv1_rx.recv().await {
                if let Lv1Command::GetState { reply } = command {
                    let _ = reply.send(connected_snapshot());
                }
            }
            let _ = candidate_lv1_closed_tx.send(());
        });
        let (candidate_fade_tx, mut candidate_fade_rx) = mpsc::channel(1);
        let started_runtime = started_runtime_for_test(
            &lifecycle,
            generation,
            runtime_generation,
            event_bus,
            test_actor_handle(candidate_lv1_tx),
            candidate_fade_tx,
            None,
        )
        .await;
        while events.try_recv().is_ok() {}

        let result = lifecycle
            .finish_connect_transaction(
                identity(Some("candidate"), Some("LV1-FOH"), "192.0.2.40"),
                started_runtime,
            )
            .await;

        assert_eq!(
            result.unwrap_err(),
            AppCommandError::ShowUnavailable.to_string()
        );
        assert!(lifecycle.current_lv1().await.is_none());
        assert!(lifecycle.current_fade().await.is_none());
        tokio::time::timeout(std::time::Duration::from_secs(1), candidate_lv1_closed_rx)
            .await
            .expect("candidate LV1 endpoint should be released")
            .expect("candidate LV1 task should report shutdown");
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), candidate_fade_rx.recv())
                .await
                .expect("candidate Fade endpoint should be released")
                .is_none()
        );
        assert!(
            capture
                .matching("lv1_connected", tracing::Level::INFO)
                .is_empty()
        );
        while let Ok(event) = events.try_recv() {
            assert!(!matches!(
                event,
                AppEvent::Lv1 {
                    event: Lv1Event::Connected,
                    ..
                }
            ));
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn closed_show_reply_after_admission_cleans_candidate_without_touching_newer_runtime() {
        assert_closed_show_reply_cleans_candidate_without_touching_newer_runtime().await;
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
            .set_lv1_connection_metadata(generation, Some(identity.clone()))
            .await
            .expect("connected metadata should apply");

        loop {
            if matches!(events.recv().await.unwrap(), AppEvent::Show(_)) {
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
            .finish_connect_transaction(identity, started_runtime)
            .await;

        assert!(matches!(result, Err(message) if message == "LV1 did not connect"));
        assert!(
            lifecycle
                .runtime_snapshot_source()
                .connected_lv1()
                .await
                .is_none()
        );
        assert_eq!(lifecycle.active_generation().await, generation + 1);
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
                    let current_generation = lifecycle_for_hook.begin_connecting().await.unwrap();
                    assert!(
                        set_show_connection(
                            &lifecycle_for_hook,
                            current_generation,
                            current_identity,
                        )
                        .await
                        .accepted
                    );
                })
            })),
        )
        .await;

        let result = lifecycle
            .finish_connect_transaction(
                identity(Some("uuid-stale"), Some("LV1-FOH"), "192.168.1.36"),
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
        assert!(
            lifecycle
                .runtime_snapshot_source()
                .connected_lv1()
                .await
                .is_none()
        );
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
                .install_runtime_transaction(installed_runtime(
                    newer_generation,
                    test_actor_handle(newer_lv1_tx),
                    mpsc::channel(1).0,
                ),)
                .await
                .is_ok(),
            "newer runtime should install"
        );
        assert!(
            set_show_connection(&lifecycle, newer_generation, newer_identity.clone())
                .await
                .accepted
        );

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
            .finish_connect_transaction(identity, started_runtime)
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
            if let AppEvent::CueLists(state) = events.recv().await.unwrap() {
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
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus);
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let unavailable_port = listener.local_addr().unwrap().port();
        drop(listener);

        let result = lifecycle
            .connect_lv1_system(Lv1SystemIdentity {
                uuid: None,
                host: Some("Unreachable".to_string()),
                address: std::net::Ipv4Addr::LOCALHOST.to_string(),
                port: unavailable_port,
            })
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
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus);
        let before = lifecycle.active_generation().await;

        let result = lifecycle
            .startup_auto_connect_lv1()
            .await
            .expect("startup without a stored identity should not fail");

        assert!(!result.changed);
        assert_eq!(lifecycle.active_generation().await, before);
    }

    #[tokio::test]
    async fn ambiguous_startup_match_preserves_generation_and_remembered_identity() {
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
            .startup_auto_connect_with_discovered(remembered.clone(), &systems)
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
            .finish_connect_transaction(identity.clone(), started_runtime)
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
                .finish_connect_transaction(identity, started_runtime)
                .await
        });

        write_received_rx
            .await
            .expect("settings write should be reached after scene-peer acceptance");
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
    async fn stale_runtime_install_is_rejected_without_becoming_current() {
        let event_bus = AppEventBus::default();
        let lifecycle = lifecycle_for_test(event_bus);
        let lv1 = fake_lv1_handle(connected_snapshot());
        let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = fade_tx;
        let runtime = installed_runtime(1, lv1, fade);

        let _rejection = lifecycle
            .install_runtime_transaction(runtime)
            .await
            .expect_err("stale generation should reject the runtime install");
        assert!(
            lifecycle
                .runtime_snapshot_source()
                .connected_lv1()
                .await
                .is_none()
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
            .install_runtime_transaction(installed_runtime(generation, lv1, fade_tx))
            .await;
        assert!(install.is_ok());

        while events.try_recv().is_ok() {}
        let result = lifecycle.disconnect_current_runtime().await.unwrap();

        assert!(result.changed);
        assert!(
            lifecycle
                .runtime_snapshot_source()
                .connected_lv1()
                .await
                .is_none()
        );
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
    async fn rejected_connection_cleanup_preserves_newer_runtime_and_scene_peers() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let lifecycle = lifecycle_for_test(event_bus);
        let rejected_generation = lifecycle.begin_connecting().await.unwrap();
        let accepted_generation = lifecycle.begin_connecting().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if matches!(events.recv().await.unwrap(), AppEvent::Scenes { generation, .. } if generation == accepted_generation) {
                    break;
                }
            }
        }).await.unwrap();
        let (newer_lv1_tx, mut newer_lv1_rx) = mpsc::channel(1);
        let (newer_fade_tx, mut newer_fade_rx) = mpsc::channel(1);
        assert!(
            lifecycle
                .install_runtime_transaction(installed_runtime(
                    accepted_generation,
                    test_actor_handle(newer_lv1_tx),
                    newer_fade_tx,
                ),)
                .await
                .is_ok(),
            "newer runtime should install"
        );
        assert!(
            lifecycle
                .install_accepted_scene_peers(accepted_generation)
                .await
        );
        let (ready_reply, ready_rx) = oneshot::channel();
        lifecycle
            .scenes_handle()
            .send(ScenesCommand::RuntimePeersReady {
                generation: accepted_generation,
                initial_scene_list: vec![],
                reply: ready_reply,
            })
            .await
            .expect("newer scene peers should accept readiness");
        ready_rx
            .await
            .expect("scene readiness reply should arrive")
            .expect("newer scene peers should become ready");

        let (rejected_lv1_tx, _rejected_lv1_rx) = mpsc::channel(1);
        let (rejected_fade_tx, _rejected_fade_rx) = mpsc::channel(1);
        lifecycle
            .abort_rejected_connection_transaction(installed_runtime(
                rejected_generation,
                test_actor_handle(rejected_lv1_tx),
                rejected_fade_tx,
            ))
            .await;

        let (snapshot_generation, snapshot_lv1) = lifecycle
            .runtime_snapshot_source()
            .connected_lv1()
            .await
            .expect("newer runtime should remain current");
        assert_eq!(snapshot_generation, accepted_generation);
        let (state_reply, state_rx) = oneshot::channel();
        snapshot_lv1
            .send(Lv1Command::GetState { reply: state_reply })
            .await
            .expect("newer LV1 mailbox should accept commands");
        let Lv1Command::GetState { reply } = newer_lv1_rx
            .recv()
            .await
            .expect("newer LV1 actor should receive the command")
        else {
            panic!("expected GetState through the newer runtime snapshot");
        };
        reply.send(connected_snapshot()).unwrap();
        assert_eq!(
            state_rx.await.unwrap().connection,
            ConnectionStatus::Connected
        );

        lifecycle
            .current_fade()
            .await
            .expect("newer fade should remain current")
            .send(crate::fade::FadeCommand::AbortAll { reply: None })
            .await
            .expect("newer fade mailbox should accept commands");
        assert!(matches!(
            newer_fade_rx.recv().await,
            Some(crate::fade::FadeCommand::AbortAll { reply: None })
        ));

        let (scenes_reply, scenes_rx) = oneshot::channel();
        lifecycle
            .scenes_handle()
            .send(ScenesCommand::InitialProjectionState {
                reply: scenes_reply,
            })
            .await
            .expect("Scenes mailbox should accept commands after stale cleanup");
        assert_eq!(
            scenes_rx.await.unwrap().ready_generation,
            Some(accepted_generation)
        );
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
            .install_runtime_transaction(installed_runtime(accepted_generation, newer_lv1, fade_tx))
            .await;
        assert!(install.is_ok());

        let stale_runtime = build_connected_runtime(
            stale_generation,
            lifecycle.current_runtime_generation().await,
            &identity(Some("uuid-stale"), Some("LV1-Stale"), "192.168.1.36"),
            event_bus.clone(),
        );
        let Err(rejection) = lifecycle
            .install_runtime_transaction(stale_runtime.runtime.clone())
            .await
        else {
            panic!("stale runtime install should be rejected");
        };
        lifecycle
            .abort_rejected_connection_transaction(rejection)
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
                .install_runtime_transaction(installed_runtime(
                    original_generation,
                    test_actor_handle(old_lv1_tx),
                    old_fade_tx,
                ),)
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
                .install_runtime_transaction(installed_runtime(
                    newer_generation,
                    test_actor_handle(newer_lv1_tx),
                    newer_fade_tx,
                ),)
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
                .install_runtime_transaction(installed_runtime(
                    generation,
                    test_actor_handle(lv1_tx),
                    fade_tx,
                ),)
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
                .install_runtime_transaction(installed_runtime(
                    newer_generation,
                    newer_lv1,
                    newer_fade_tx,
                ),)
                .await
                .is_ok(),
            "newer runtime should install"
        );
        let newer_identity = identity(Some("newer"), Some("LV1-FOH"), "192.0.2.20");
        assert!(
            set_show_connection(&lifecycle, newer_generation, newer_identity.clone())
                .await
                .accepted
        );
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
        assert!(
            set_show_connection(&lifecycle, generation, connected_identity)
                .await
                .accepted
        );
        loop {
            if matches!(rx.recv().await.unwrap(), AppEvent::Show(_)) {
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

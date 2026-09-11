use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, watch};

use crate::cue_lists::CueListDocument;
use crate::lv1::{Lv1ActorError, Lv1ActorHandle, Lv1Command, Lv1StateSnapshot};
use crate::runtime::errors::AppCommandError;
use crate::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use crate::runtime::generation::RuntimeGeneration;
use crate::scenes::{SceneDocument, ScenesCommand, ScenesHandle};
use crate::session::{SessionDocument, SessionReplacement};
use crate::show_file::{backup_folder, read_show_file, write_show_file};

use super::commands::ShowCommand;
use super::handle::ShowStateHandle;
use super::lockout::ShowLockoutReader;
use super::show_file::import_show_file;
use super::state::ShowState;
use super::{LoadShowFileResult, NewShowFileResult, ShowCommandResult};

const SHOW_LOCAL_ACTOR_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Default)]
pub struct ShowActorPeers {
    runtime_generation: RuntimeGeneration,
    lv1: Arc<Mutex<Option<(u64, Lv1ActorHandle)>>>,
    scenes: Arc<Mutex<Option<ScenesHandle>>>,
}

impl ShowActorPeers {
    pub(crate) fn runtime_generation(&self) -> RuntimeGeneration {
        self.runtime_generation.clone()
    }

    pub fn set_lv1(&self, generation: u64, lv1: Lv1ActorHandle) {
        *self.lv1.lock().expect("show peer lock poisoned") = Some((generation, lv1));
    }

    pub fn set_scenes(&self, scenes: ScenesHandle) {
        *self.scenes.lock().expect("show peer lock poisoned") = Some(scenes);
    }

    pub fn clear_lv1(&self, generation: u64) {
        let mut lv1 = self.lv1.lock().expect("show peer lock poisoned");
        if lv1
            .as_ref()
            .is_some_and(|(peer_generation, _)| *peer_generation == generation)
        {
            *lv1 = None;
        }
    }

    fn lv1(&self) -> Option<(u64, Lv1ActorHandle)> {
        self.lv1
            .lock()
            .expect("show peer lock poisoned")
            .as_ref()
            .map(|(generation, lv1)| (*generation, lv1.clone()))
    }

    pub fn scenes(&self) -> Option<ScenesHandle> {
        self.scenes
            .lock()
            .expect("show peer lock poisoned")
            .as_ref()
            .cloned()
    }
}

pub struct ShowActorTask {
    rx: mpsc::Receiver<ShowCommand>,
    events: tokio::sync::broadcast::Receiver<AppEvent>,
    event_bus: AppEventBus,
    peers: ShowActorPeers,
    state: ShowState,
    lockout_tx: watch::Sender<bool>,
    backup_dir: std::path::PathBuf,
}

impl ShowActorTask {
    pub fn spawn(self) {
        tauri::async_runtime::spawn(run_show_actor(
            self.rx,
            self.events,
            self.event_bus,
            self.peers,
            self.state,
            self.lockout_tx,
            self.backup_dir,
        ));
    }

    #[cfg(test)]
    fn with_backup_dir(mut self, backup_dir: std::path::PathBuf) -> Self {
        self.backup_dir = backup_dir;
        self
    }
}

pub fn build_show_actor(
    event_bus: AppEventBus,
) -> (
    ShowStateHandle,
    ShowActorTask,
    ShowActorPeers,
    ShowLockoutReader,
) {
    build_show_actor_with_state(event_bus, ShowState::default())
}

fn build_show_actor_with_state(
    event_bus: AppEventBus,
    state: ShowState,
) -> (
    ShowStateHandle,
    ShowActorTask,
    ShowActorPeers,
    ShowLockoutReader,
) {
    event_bus.retain(&AppEvent::Show(state.projection_state()));
    let (tx, rx) = mpsc::channel(32);
    let (lockout_tx, lockout_rx) = watch::channel(state.lockout());
    let peers = ShowActorPeers::default();
    let task = ShowActorTask {
        rx,
        events: event_bus.subscribe(),
        event_bus,
        peers: peers.clone(),
        state,
        lockout_tx,
        backup_dir: backup_folder(),
    };
    (tx, task, peers, ShowLockoutReader::new(lockout_rx))
}

async fn run_show_actor(
    mut rx: mpsc::Receiver<ShowCommand>,
    mut events: tokio::sync::broadcast::Receiver<AppEvent>,
    event_bus: AppEventBus,
    peers: ShowActorPeers,
    mut state: ShowState,
    lockout_tx: watch::Sender<bool>,
    backup_dir: std::path::PathBuf,
) {
    loop {
        tokio::select! {
            command = rx.recv() => {
                let Some(command) = command else { break; };
                handle_command(command, &mut state, &event_bus, &peers, &backup_dir).await;
                publish_lockout_if_changed(&lockout_tx, &state);
            }
            event = events.recv() => {
                match event {
                    Ok(event) => handle_app_event(event, &mut state, &event_bus),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        log_lagged_subscriber("show-actor", count);
                        state.mark_dirty();
                        publish_state_changed(&event_bus, &state);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

fn publish_lockout_if_changed(lockout_tx: &watch::Sender<bool>, state: &ShowState) {
    lockout_tx.send_if_modified(|current| {
        let next = state.lockout();
        let changed = *current != next;
        *current = next;
        changed
    });
}

fn handle_app_event(event: AppEvent, state: &mut ShowState, event_bus: &AppEventBus) {
    match event {
        AppEvent::Scenes {
            generation: _,
            event:
                crate::scenes::ScenesEvent::StateChanged {
                    persisted_scene_edit: true,
                    ..
                },
        }
        | AppEvent::CueLists(_) => {
            state.mark_dirty();
            publish_state_changed(event_bus, state);
        }
        _ => {}
    }
}

fn publish_state_changed(event_bus: &AppEventBus, state: &ShowState) {
    event_bus.publish(AppEvent::Show(state.projection_state()));
}

fn publish_if_changed(event_bus: &AppEventBus, state: &ShowState, changed: bool) {
    if changed {
        publish_state_changed(event_bus, state);
    }
}

async fn handle_command(
    command: ShowCommand,
    state: &mut ShowState,
    event_bus: &AppEventBus,
    peers: &ShowActorPeers,
    backup_dir: &std::path::Path,
) {
    match command {
        ShowCommand::CurrentShowFilePath { reply } => {
            let _ = reply.send(state.current_show_file_path());
        }
        ShowCommand::InitialProjectionState { reply } => {
            let _ = reply.send(state.projection_state());
        }
        ShowCommand::SetLockout { enabled, reply } => {
            let changed = state.set_lockout(enabled);
            publish_if_changed(event_bus, state, changed);
            if let Some(reply) = reply {
                let _ = reply.send(ShowCommandResult { changed });
            }
        }
        ShowCommand::NewShowFileFromCurrentLv1 { reply } => {
            let result = async {
                let (expected_generation, lv1) = current_lv1_snapshot(peers).await?;
                let scene_document = SceneDocument {
                    scene_configs: crate::scenes::align_scene_configs(Vec::new(), &lv1.scene_list),
                    selected_scene_internal_id: None,
                };
                let selected_scene_internal_id = scene_document
                    .scene_configs
                    .first()
                    .map(|scene| scene.internal_scene_id.to_string());
                validate_lv1_snapshot(peers, expected_generation, &lv1).await?;
                replace_session_document(
                    peers,
                    SessionDocument {
                        scenes: scene_document,
                        cue_lists: CueListDocument::default(),
                    },
                    expected_generation,
                )
                .await?;
                state.reset_for_new_show();
                publish_state_changed(event_bus, state);
                tracing::info!(event = "session_created", "New session created");
                Ok(NewShowFileResult {
                    selected_scene_internal_id,
                })
            }
            .await;
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::SaveShowFileAs { path, reply } => {
            let result = async {
                let saved_at = crate::time::current_timestamp_millis();
                let document = current_session_document(peers).await?;
                let file = crate::show::show_file::export_show_file(
                    document.scenes,
                    document.cue_lists,
                    state.lockout(),
                    saved_at.clone(),
                );
                write_show_file(&path, &file, backup_dir)?;
                state.mark_saved(path, saved_at);
                publish_state_changed(event_bus, state);
                tracing::info!(event = "session_saved", "Session saved");
                Ok(ShowCommandResult { changed: true })
            }
            .await;
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::SetDiscoveredLv1Systems { systems, reply } => {
            let changed = state.set_discovered_lv1_systems(systems);
            publish_if_changed(event_bus, state, changed);
            if let Some(reply) = reply {
                let _ = reply.send(ShowCommandResult { changed });
            }
        }
        ShowCommand::SetLv1ConnectionIfCurrent {
            identity,
            expected_generation,
            reply,
        } => {
            let outcome = peers
                .runtime_generation
                .if_current(expected_generation, || {
                    let changed = state.set_lv1_connection(identity);
                    publish_if_changed(event_bus, state, changed);
                    super::CompleteConnectionOutcome {
                        accepted: true,
                        changed,
                    }
                })
                .await
                .unwrap_or(super::CompleteConnectionOutcome {
                    accepted: false,
                    changed: false,
                });
            let _ = reply.send(outcome);
        }
        ShowCommand::LoadShowFileFromPath { path, reply } => {
            let result = async {
                let (expected_generation, lv1) = current_lv1_snapshot(peers).await?;
                let mut file = read_show_file(&path)?;
                load_show_file_from_dto(
                    state,
                    event_bus,
                    peers,
                    path,
                    &mut file,
                    &lv1,
                    expected_generation,
                )
                .await
            }
            .await;
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
    }
}

async fn current_lv1_snapshot(peers: &ShowActorPeers) -> Result<(u64, Lv1StateSnapshot), String> {
    let (generation, lv1) = peers
        .lv1()
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let snapshot = get_lv1_state(&lv1).await?;
    if snapshot.connection != crate::lv1::ConnectionStatus::Connected {
        return Err(AppCommandError::Lv1Unavailable.to_string());
    }
    Ok((generation, snapshot))
}

async fn get_lv1_state(lv1: &Lv1ActorHandle) -> Result<Lv1StateSnapshot, String> {
    let (reply, rx) = tokio::sync::oneshot::channel();
    tokio::time::timeout(
        SHOW_LOCAL_ACTOR_TIMEOUT,
        lv1.send(Lv1Command::GetState { reply }),
    )
    .await
    .map_err(|_| "LV1 state request timed out".to_string())?
    .map_err(|error| match error {
        Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
        other => AppCommandError::CommandFailed(other.to_string()),
    })
    .map_err(map_app_command_error)?;
    tokio::time::timeout(SHOW_LOCAL_ACTOR_TIMEOUT, rx)
        .await
        .map_err(|_| "LV1 state reply timed out".to_string())?
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)
}

async fn validate_lv1_snapshot(
    peers: &ShowActorPeers,
    expected_generation: u64,
    expected_snapshot: &Lv1StateSnapshot,
) -> Result<(), String> {
    let (peer_generation, lv1) = peers
        .lv1()
        .ok_or_else(|| "LV1 generation is no longer current".to_string())?;
    if peer_generation != expected_generation {
        return Err("LV1 generation is no longer current".to_string());
    }
    let snapshot = get_lv1_state(&lv1).await?;
    if snapshot.connection != crate::lv1::ConnectionStatus::Connected {
        return Err("LV1 is no longer connected".to_string());
    }
    if snapshot.scene_list != expected_snapshot.scene_list {
        return Err("LV1 scene list changed during show replacement".to_string());
    }
    Ok(())
}

fn map_app_command_error(error: AppCommandError) -> String {
    match error {
        AppCommandError::CommandFailed(message) => message,
        other => other.to_string(),
    }
}

async fn load_show_file_from_dto(
    state: &mut ShowState,
    event_bus: &AppEventBus,
    peers: &ShowActorPeers,
    path: std::path::PathBuf,
    file: &mut super::show_file::ShowFile,
    lv1: &Lv1StateSnapshot,
    expected_generation: u64,
) -> Result<LoadShowFileResult, String> {
    let imported = import_show_file(file, lv1)?;
    let saved_at = file.saved_at.clone();
    let selected_scene_internal_id = imported.selected_scene_internal_id.clone();
    let imported_scene_configs = imported.snapshot.scene_configs;
    let imported_cue_list_snapshot = imported.cue_list_snapshot.clone();
    let aligned_scene_configs =
        crate::scenes::align_scene_configs(imported_scene_configs.clone(), &lv1.scene_list);
    let alignment_changed = aligned_scene_configs != imported_scene_configs;
    let mut should_mark_dirty = imported.generated_internal_scene_ids || alignment_changed;
    let selected_scene_internal_id = selected_scene_internal_id
        .filter(|selected| {
            aligned_scene_configs
                .iter()
                .any(|scene| scene.internal_scene_id.to_string() == *selected)
        })
        .or_else(|| {
            aligned_scene_configs
                .first()
                .map(|scene| scene.internal_scene_id.to_string())
        });
    let scene_document = SceneDocument {
        scene_configs: aligned_scene_configs.clone(),
        selected_scene_internal_id: selected_scene_internal_id.clone(),
    };
    validate_lv1_snapshot(peers, expected_generation, lv1).await?;
    let committed = replace_session_document(
        peers,
        SessionDocument {
            scenes: scene_document,
            cue_lists: imported.cue_list_snapshot,
        },
        expected_generation,
    )
    .await?;
    should_mark_dirty |= committed.cue_lists != imported_cue_list_snapshot;
    state.set_lockout(imported.lockout);
    state.mark_saved(path, saved_at.clone());
    if should_mark_dirty {
        state.mark_dirty();
    }
    publish_state_changed(event_bus, state);
    if alignment_changed {
        tracing::debug!(
            event = "session_scene_alignment",
            "{}",
            crate::scenes::scene_alignment_diagnostic(
                &imported_scene_configs,
                &aligned_scene_configs,
                &lv1.scene_list
            )
        );
    }
    tracing::info!(event = "session_opened", "Session loaded");
    Ok(LoadShowFileResult {
        selected_scene_internal_id,
        saved_at,
    })
}

async fn current_session_document(peers: &ShowActorPeers) -> Result<SessionDocument, String> {
    let scenes = peers
        .scenes()
        .ok_or_else(|| "Show blocked: scenes state is unavailable".to_string())?;
    let (reply, response) = tokio::sync::oneshot::channel();
    tokio::time::timeout(
        SHOW_LOCAL_ACTOR_TIMEOUT,
        scenes.send(ScenesCommand::GetSessionDocument { reply }),
    )
    .await
    .map_err(|_| "Session snapshot request timed out".to_string())?
    .map_err(|_| "Show blocked: scenes state is unavailable".to_string())?;
    tokio::time::timeout(SHOW_LOCAL_ACTOR_TIMEOUT, response)
        .await
        .map_err(|_| "Session snapshot reply timed out".to_string())?
        .map_err(|_| "Show blocked: scenes state is unavailable".to_string())
}

async fn replace_session_document(
    peers: &ShowActorPeers,
    document: SessionDocument,
    expected_generation: u64,
) -> Result<SessionDocument, String> {
    let scenes = peers
        .scenes()
        .ok_or_else(|| "Show blocked: scenes state is unavailable".to_string())?;
    let replacement = SessionReplacement::new(document);
    let (reply, response) = tokio::sync::oneshot::channel();
    tokio::time::timeout(
        SHOW_LOCAL_ACTOR_TIMEOUT,
        scenes.send(ScenesCommand::ReplaceSessionDocument {
            replacement: replacement.clone(),
            expected_generation,
            reply,
        }),
    )
    .await
    .map_err(|_| "Session replacement request timed out".to_string())?
    .map_err(|_| "Show blocked: scenes state is unavailable".to_string())?;
    match tokio::time::timeout(SHOW_LOCAL_ACTOR_TIMEOUT, response).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => replacement
            .cancel_or_committed()
            .map_err(|_| "Show blocked: scenes state is unavailable".to_string()),
        Err(_) => replacement.cancel_or_committed(),
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::build_show_actor;
    use crate::cue_lists::{CueListDocument, CueListsCommand, CueListsProjectionState};
    use crate::lv1::{ConnectionStatus, Lv1StateSnapshot, SceneListEntry};
    use crate::runtime::events::{AppEventBus, RuntimeLifecycleEvent};
    use crate::runtime::generation::RuntimeGeneration;
    use crate::scenes::{SceneConfig, SceneScopeToggles};
    use crate::scenes::{ScenesCommand, build_scenes_actor};
    use crate::settings::{AppSettings, SettingsCommand, SettingsHandle};
    use crate::show::commands::ShowCommand;
    use crate::show::handle::ShowStateHandle;
    use crate::show::{ShowFile, ShowFileSafety, ShowFileSceneConfig};

    fn lv1_snapshot(scenes: Vec<SceneListEntry>) -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: scenes,
            channels: Vec::new(),
            ping_sequence: 0,
        }
    }

    fn show_file(scenes: Vec<ShowFileSceneConfig>) -> ShowFile {
        ShowFile {
            schema_version: crate::show::SHOW_FILE_SCHEMA_VERSION,
            app_version: "test".to_string(),
            saved_at: "123".to_string(),
            safety: ShowFileSafety { lockout: false },
            scene_configs: scenes,
            cue_lists: Vec::new(),
            active_cue_list_id: None,
            cued_cue_entry_id: None,
        }
    }

    fn scene_config(id: u128, index: Option<i32>, name: &str, duration_ms: u64) -> SceneConfig {
        SceneConfig {
            internal_scene_id: Uuid::from_u128(id),
            scene_index: index,
            scene_name: name.to_string(),
            duration_ms,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    fn file_scene(config: SceneConfig) -> ShowFileSceneConfig {
        ShowFileSceneConfig {
            internal_scene_id: Some(config.internal_scene_id),
            scene_index: config.scene_index,
            scene_name: config.scene_name,
            duration_ms: config.duration_ms,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: Default::default(),
        }
    }

    fn show_actor_peers() -> (super::ShowActorPeers, crate::cue_lists::CueListsHandle) {
        let peers = super::ShowActorPeers::default();
        let event_bus = AppEventBus::default();
        let (scenes, task, _peers) = build_scenes_actor(
            0,
            peers.runtime_generation(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            test_lockout_reader(),
        );
        let cue_lists = task.cue_lists_handle();
        task.spawn();
        peers.set_scenes(scenes);
        (peers, cue_lists)
    }

    fn test_lockout_reader() -> super::ShowLockoutReader {
        let (_sender, receiver) = tokio::sync::watch::channel(false);
        super::ShowLockoutReader::new(receiver)
    }

    fn show_actor(event_bus: AppEventBus) -> (ShowStateHandle, super::ShowActorPeers) {
        let (handle, task, peers, _lockout) = build_show_actor(event_bus);
        task.spawn();
        (handle, peers)
    }

    fn load_fixture(
        event_bus: AppEventBus,
        snapshot: Lv1StateSnapshot,
        advance_after_first_snapshot: bool,
    ) -> (
        ShowStateHandle,
        super::ShowActorPeers,
        crate::scenes::ScenesHandle,
        crate::cue_lists::CueListsHandle,
    ) {
        let (show, task, peers, lockout) = build_show_actor(event_bus.clone());
        let generation = peers.runtime_generation();
        let advance_generation = generation.clone();
        let (scenes, scenes_task, _scenes_peers) = build_scenes_actor(
            0,
            generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            lockout,
        );
        let cue_lists = scenes_task.cue_lists_handle();
        peers.set_scenes(scenes.clone());
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            let mut first = true;
            while let Some(command) = lv1_rx.recv().await {
                if let crate::lv1::Lv1Command::GetState { reply } = command {
                    if first {
                        first = false;
                        if advance_after_first_snapshot {
                            advance_generation.advance().await;
                        }
                    }
                    let _ = reply.send(snapshot.clone());
                }
            }
        });
        peers.set_lv1(0, crate::lv1::test_actor_handle(lv1_tx));
        task.spawn();
        scenes_task.spawn();
        (show, peers, scenes, cue_lists)
    }

    fn write_test_show(name: &str, file: &ShowFile) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("show-{name}-{}.ascs", Uuid::new_v4()));
        std::fs::write(&path, serde_json::to_vec_pretty(file).unwrap()).unwrap();
        path
    }

    async fn load_show(
        show: &ShowStateHandle,
        path: std::path::PathBuf,
    ) -> Result<crate::show::LoadShowFileResult, String> {
        let (reply, response) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::LoadShowFileFromPath {
            path,
            reply: Some(reply),
        })
        .await
        .unwrap();
        response.await.unwrap()
    }

    async fn current_show_state(
        show: &ShowStateHandle,
    ) -> crate::show::events::ShowProjectionState {
        let (reply, response) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        response.await.unwrap()
    }

    fn save_fixture(
        backup_dir: std::path::PathBuf,
    ) -> (ShowStateHandle, crate::scenes::ScenesHandle) {
        let event_bus = AppEventBus::default();
        let (show, task, peers, lockout) = build_show_actor(event_bus.clone());
        let (scenes, scenes_task, _scenes_peers) = build_scenes_actor(
            0,
            peers.runtime_generation(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            lockout,
        );
        peers.set_scenes(scenes.clone());
        task.with_backup_dir(backup_dir).spawn();
        scenes_task.spawn();
        (show, scenes)
    }

    async fn save_show(show: &ShowStateHandle, path: std::path::PathBuf) -> Result<(), String> {
        let (reply, response) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::SaveShowFileAs {
            path,
            reply: Some(reply),
        })
        .await
        .unwrap();
        response.await.unwrap().map(|_| ())
    }

    struct TestDir(std::path::PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("show-save-{name}-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn backup_entries(backup_dir: &std::path::Path, stem: &str) -> Vec<std::path::PathBuf> {
        std::fs::read_dir(backup_dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    return false;
                };
                let published = name.ends_with(&format!("-{stem}.ascs"))
                    || name.contains(&format!("-{stem}__backup"));
                let staged = name.starts_with('.') && name.contains(&format!("-{stem}"));
                published || staged
            })
            .collect()
    }

    fn fake_settings_handle() -> SettingsHandle {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if let SettingsCommand::GetSettings { reply } = command {
                    let _ = reply.send(AppSettings::default());
                }
            }
        });
        tx
    }

    async fn get_scene_document(
        handle: &crate::scenes::ScenesHandle,
    ) -> crate::scenes::SceneDocument {
        let (reply, rx) = tokio::sync::oneshot::channel();
        handle
            .send(ScenesCommand::GetSessionDocument { reply })
            .await
            .unwrap();
        rx.await.unwrap().scenes
    }

    async fn get_cue_list_document(
        handle: &crate::cue_lists::CueListsHandle,
    ) -> crate::cue_lists::CueListDocument {
        let (reply, rx) = tokio::sync::oneshot::channel();
        handle
            .send(CueListsCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        rx.await.unwrap().document
    }

    async fn recv_file_metadata_event(
        events: &mut tokio::sync::broadcast::Receiver<crate::runtime::events::AppEvent>,
    ) -> crate::show::events::ShowProjectionState {
        loop {
            match events.recv().await.unwrap() {
                crate::runtime::events::AppEvent::Show(state) => {
                    return state;
                }
                _ => continue,
            }
        }
    }

    #[tokio::test]
    async fn clear_current_connection_publishes_metadata_state_change() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let (show, _peers) = show_actor(event_bus);
        let identity = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1".to_string()),
            address: "127.0.0.1".to_string(),
            port: 50_000,
        };
        let (reply, response) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::SetLv1ConnectionIfCurrent {
            identity: Some(identity),
            expected_generation: 0,
            reply,
        })
        .await
        .unwrap();
        assert!(response.await.unwrap().changed);
        let _ = events.recv().await.unwrap();

        let (reply, response) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::SetLv1ConnectionIfCurrent {
            identity: None,
            expected_generation: 0,
            reply,
        })
        .await
        .unwrap();
        assert!(response.await.unwrap().changed);
        assert!(matches!(
            events.recv().await.unwrap(),
            crate::runtime::events::AppEvent::Show(state) if state.connected_lv1_identity.is_none()
        ));
    }

    #[tokio::test]
    async fn new_show_rejects_lv1_change_before_mutating_documents() {
        let event_bus = AppEventBus::default();
        let (show, peers) = show_actor(event_bus);
        let (fixture, cue_lists) = show_actor_peers();
        peers.set_scenes(fixture.scenes().unwrap());
        let first = lv1_snapshot(vec![SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }]);
        let mut second = first.clone();
        second.connection = ConnectionStatus::Disconnected;
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(4);
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        tokio::spawn(async move {
            let mut snapshots = [first, second].into_iter();
            while let Some(command) = lv1_rx.recv().await {
                if let crate::lv1::Lv1Command::GetState { reply } = command {
                    let _ = reply.send(snapshots.next().unwrap());
                }
            }
        });
        peers.set_lv1(0, lv1);

        let (reply, response) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::NewShowFileFromCurrentLv1 { reply: Some(reply) })
            .await
            .unwrap();
        assert_eq!(
            response.await.unwrap().unwrap_err(),
            "LV1 is no longer connected"
        );
        assert!(
            get_scene_document(&peers.scenes().unwrap())
                .await
                .scene_configs
                .is_empty()
        );
        assert_eq!(
            get_cue_list_document(&cue_lists).await,
            CueListDocument::default()
        );
    }

    #[tokio::test]
    async fn stale_load_rejects_before_mutating_either_document() {
        let event_bus = AppEventBus::default();
        let snapshot = lv1_snapshot(vec![SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }]);
        let (show, _peers, scenes, cue_lists) = load_fixture(event_bus, snapshot, true);
        let path = write_test_show(
            "stale",
            &show_file(vec![file_scene(scene_config(1, Some(1), "Intro", 0))]),
        );

        let result = load_show(&show, path.clone()).await;

        assert_eq!(result.unwrap_err(), "LV1 generation is no longer current");
        assert!(get_scene_document(&scenes).await.scene_configs.is_empty());
        assert_eq!(
            get_cue_list_document(&cue_lists).await,
            CueListDocument::default()
        );
        let state = current_show_state(&show).await;
        assert_eq!(state.show_file_name, "Untitled Session");
        assert!(!state.show_file_dirty);
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn load_rejects_missing_lv1_peer_without_mutating_documents() {
        let event_bus = AppEventBus::default();
        let (show, task, peers, lockout) = build_show_actor(event_bus.clone());
        let (scenes, scenes_task, _scenes_peers) = build_scenes_actor(
            0,
            peers.runtime_generation(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            lockout,
        );
        let cue_lists = scenes_task.cue_lists_handle();
        peers.set_scenes(scenes.clone());
        task.spawn();
        scenes_task.spawn();
        let path = write_test_show(
            "missing-lv1",
            &show_file(vec![file_scene(scene_config(1, Some(1), "Intro", 0))]),
        );

        let result = load_show(&show, path.clone()).await;

        assert_eq!(result.unwrap_err(), "LV1 actor is unavailable");
        assert!(get_scene_document(&scenes).await.scene_configs.is_empty());
        assert_eq!(
            get_cue_list_document(&cue_lists).await,
            CueListDocument::default()
        );
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn connected_load_aligns_imported_configs_and_adds_default_linked_configs_for_extra_lv1_scenes()
     {
        let event_bus = AppEventBus::default();
        let (show, _peers, scenes, _cue_lists) = load_fixture(
            event_bus,
            lv1_snapshot(vec![
                SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                },
                SceneListEntry {
                    index: 2,
                    name: "Verse".to_string(),
                },
            ]),
            false,
        );
        let path = write_test_show(
            "alignment",
            &show_file(vec![file_scene(scene_config(1, Some(1), "Intro", 1_000))]),
        );

        let result = load_show(&show, path.clone())
            .await
            .expect("load should succeed");

        let document = get_scene_document(&scenes).await;
        assert_eq!(document.scene_configs.len(), 2);
        assert_eq!(document.scene_configs[0].scene_index, Some(1));
        assert_eq!(document.scene_configs[0].duration_ms, 1_000);
        assert_eq!(document.scene_configs[1].scene_index, Some(2));
        assert_eq!(document.scene_configs[1].scene_name, "Verse");
        assert_eq!(document.scene_configs[1].duration_ms, 0);
        assert_eq!(
            result.selected_scene_internal_id,
            Some(Uuid::from_u128(1).to_string())
        );
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn connected_load_preserves_default_scene_ids_referenced_by_cue_entries() {
        let event_bus = AppEventBus::default();
        let (show, _peers, scenes, cue_lists) = load_fixture(
            event_bus,
            lv1_snapshot(vec![
                SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                },
                SceneListEntry {
                    index: 2,
                    name: "Verse".to_string(),
                },
            ]),
            false,
        );
        let intro_id = Uuid::from_u128(1);
        let verse_id = Uuid::from_u128(2);
        let cue_list_id = Uuid::from_u128(3);
        let intro_entry_id = Uuid::from_u128(4);
        let verse_entry_id = Uuid::from_u128(5);
        let mut file = show_file(vec![
            file_scene(scene_config(1, Some(1), "Intro", 0)),
            file_scene(scene_config(2, Some(2), "Verse", 0)),
        ]);
        file.cue_lists = vec![crate::cue_lists::CueList {
            id: cue_list_id,
            name: "Main".to_string(),
            entries: vec![
                crate::cue_lists::CueEntry {
                    id: intro_entry_id,
                    scene_internal_id: intro_id,
                },
                crate::cue_lists::CueEntry {
                    id: verse_entry_id,
                    scene_internal_id: verse_id,
                },
            ],
        }];
        file.active_cue_list_id = Some(cue_list_id);
        file.cued_cue_entry_id = Some(intro_entry_id);
        let path = write_test_show("cue-ids", &file);

        load_show(&show, path.clone())
            .await
            .expect("load should succeed");

        let scene_document = get_scene_document(&scenes).await;
        assert_eq!(scene_document.scene_configs[0].internal_scene_id, intro_id);
        assert_eq!(scene_document.scene_configs[1].internal_scene_id, verse_id);
        let cue_document = get_cue_list_document(&cue_lists).await;
        assert_eq!(cue_document.cue_lists[0].entries.len(), 2);
        assert_eq!(
            cue_document.cue_lists[0].entries[0].scene_internal_id,
            intro_id
        );
        assert_eq!(
            cue_document.cue_lists[0].entries[1].scene_internal_id,
            verse_id
        );
        assert_eq!(cue_document.cued_cue_entry_id, Some(intro_entry_id));
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn connected_load_marks_dirty_when_alignment_changes_imported_configs() {
        let event_bus = AppEventBus::default();
        let (show, _peers, scenes, _cue_lists) = load_fixture(
            event_bus,
            lv1_snapshot(vec![SceneListEntry {
                index: 2,
                name: "Intro".to_string(),
            }]),
            false,
        );
        let path = write_test_show(
            "dirty-alignment",
            &show_file(vec![file_scene(scene_config(1, Some(1), "Intro", 1_000))]),
        );

        load_show(&show, path.clone())
            .await
            .expect("load should succeed");

        assert!(current_show_state(&show).await.show_file_dirty);
        assert_eq!(
            get_scene_document(&scenes).await.scene_configs[0].scene_index,
            Some(2)
        );
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn connected_load_preserves_existing_fade_data_and_missing_config_as_unlinked() {
        let event_bus = AppEventBus::default();
        let (show, _peers, scenes, _cue_lists) = load_fixture(
            event_bus,
            lv1_snapshot(vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }]),
            false,
        );
        let path = write_test_show(
            "fade-and-missing",
            &show_file(vec![
                file_scene(scene_config(1, Some(1), "Intro", 1_500)),
                file_scene(scene_config(2, Some(2), "Verse", 2_000)),
            ]),
        );

        load_show(&show, path.clone())
            .await
            .expect("load should succeed");

        let document = get_scene_document(&scenes).await;
        assert_eq!(document.scene_configs.len(), 2);
        assert_eq!(document.scene_configs[0].duration_ms, 1_500);
        assert_eq!(
            document.scene_configs[0].internal_scene_id,
            Uuid::from_u128(1)
        );
        assert_eq!(document.scene_configs[1].scene_index, None);
        assert_eq!(document.scene_configs[1].scene_name, "Verse");
        assert_eq!(document.scene_configs[1].duration_ms, 2_000);
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn connected_load_clears_missing_cued_entry_but_keeps_the_cue_list_entry() {
        let event_bus = AppEventBus::default();
        let (show, _peers, _scenes, cue_lists) = load_fixture(
            event_bus,
            lv1_snapshot(vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }]),
            false,
        );
        let cue_list_id = Uuid::from_u128(0x55555555555545558555555555555555);
        let entry_id = Uuid::from_u128(0x66666666666646668666666666666666);
        let missing_scene_id = Uuid::from_u128(0x77777777777747778777777777777777);
        let mut file = show_file(vec![file_scene(scene_config(1, Some(1), "Intro", 1_000))]);
        file.cue_lists = vec![crate::cue_lists::CueList {
            id: cue_list_id,
            name: "Main".to_string(),
            entries: vec![crate::cue_lists::CueEntry {
                id: entry_id,
                scene_internal_id: missing_scene_id,
            }],
        }];
        file.active_cue_list_id = Some(cue_list_id);
        file.cued_cue_entry_id = Some(entry_id);
        let path = write_test_show("missing-cued-entry", &file);

        load_show(&show, path.clone())
            .await
            .expect("load should succeed");

        let cue_document = get_cue_list_document(&cue_lists).await;
        assert_eq!(cue_document.cue_lists[0].entries.len(), 1);
        assert_eq!(cue_document.cue_lists[0].entries[0].id, entry_id);
        assert_eq!(cue_document.cued_cue_entry_id, None);
        assert!(current_show_state(&show).await.show_file_dirty);
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn save_roundtrips_session_and_creates_missing_parent_directories() {
        let root = TestDir::new("roundtrip");
        let backup_dir = root.path().join("backups");
        let (show, scenes) = save_fixture(backup_dir);
        crate::session::tests::replace_scenes(
            &scenes,
            crate::scenes::SceneDocument {
                scene_configs: vec![scene_config(21, Some(4), "Roundtrip", 3_500)],
                selected_scene_internal_id: None,
            },
            0,
        )
        .await;
        let path = root
            .path()
            .join("missing")
            .join("nested")
            .join("roundtrip.ascs");

        save_show(&show, path.clone()).await.unwrap();

        let saved = crate::show_file::read_show_file(&path).unwrap();
        assert_eq!(saved.scene_configs[0].scene_name, "Roundtrip");
        assert_eq!(saved.scene_configs[0].scene_index, Some(4));
        assert_eq!(saved.scene_configs[0].duration_ms, 3_500);
        assert!(path.parent().unwrap().is_dir());
    }

    #[tokio::test]
    async fn overwrite_backs_up_prior_contents_and_repeated_saves_use_unique_names() {
        let root = TestDir::new("backups");
        let backup_dir = root.path().join("backups");
        let (show, _scenes) = save_fixture(backup_dir.clone());
        let stem = format!("backup-{}", Uuid::new_v4());
        let path = root.path().join(format!("{stem}.ascs"));
        std::fs::write(&path, "prior session contents").unwrap();

        save_show(&show, path.clone()).await.unwrap();
        save_show(&show, path.clone()).await.unwrap();

        let backups = backup_entries(&backup_dir, &stem);
        assert_eq!(backups.len(), 2);
        assert!(
            backups
                .iter()
                .all(|path| { !path.file_name().unwrap().to_string_lossy().starts_with('.') })
        );
        assert!(
            backups
                .iter()
                .any(|backup| std::fs::read_to_string(backup).unwrap() == "prior session contents")
        );
    }

    #[tokio::test]
    async fn save_retention_prunes_only_exact_show_backups() {
        let root = TestDir::new("retention");
        let backup_dir = root.path().join("backups");
        let (show, _scenes) = save_fixture(backup_dir.clone());
        let stem = format!("retention-{}", Uuid::new_v4());
        let neighbor_stem = format!("{stem}-neighbor");
        let unrelated_stem = format!("unrelated-{}", Uuid::new_v4());
        let path = root.path().join(format!("{stem}.ascs"));
        std::fs::create_dir_all(&backup_dir).unwrap();
        for index in 0..11 {
            std::fs::write(backup_dir.join(format!("100{index}-{stem}.ascs")), "old").unwrap();
        }
        let neighbor = backup_dir.join(format!("2000-{neighbor_stem}.ascs"));
        let unrelated = backup_dir.join(format!("2000-{unrelated_stem}.ascs"));
        let staged = backup_dir.join(format!(".2000-{stem}.ascs.tmp"));
        std::fs::write(&neighbor, "neighbor").unwrap();
        std::fs::write(&unrelated, "unrelated").unwrap();
        std::fs::write(&staged, "staged").unwrap();
        std::fs::write(&path, "current").unwrap();

        save_show(&show, path.clone()).await.unwrap();

        assert_eq!(
            backup_entries(&backup_dir, &stem)
                .iter()
                .filter(|path| !path.file_name().unwrap().to_string_lossy().starts_with('.'))
                .count(),
            10
        );
        assert_eq!(std::fs::read_to_string(&staged).unwrap(), "staged");
        assert_eq!(std::fs::read_to_string(&neighbor).unwrap(), "neighbor");
        assert_eq!(std::fs::read_to_string(&unrelated).unwrap(), "unrelated");
    }

    #[tokio::test]
    async fn failed_save_preserves_original_and_leaves_no_staged_files() {
        let root = TestDir::new("failure");
        let backup_dir = root.path().join("backups");
        let (show, _scenes) = save_fixture(backup_dir.clone());
        let stem = format!("failure-{}", Uuid::new_v4());
        let path = root.path().join(format!("{stem}.ascs"));
        std::fs::create_dir(&path).unwrap();

        let result = save_show(&show, path.clone()).await;

        assert!(result.is_err());
        assert!(path.is_dir());
        assert!(backup_entries(&backup_dir, &stem).is_empty());
        assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(&format!(".{stem}.ascs.tmp-"))
        }));
    }

    #[tokio::test]
    async fn load_replaces_scenes_state_and_keeps_dirty_clear_for_replacement_event() {
        let event_bus = AppEventBus::default();
        let (show, peers) = show_actor(event_bus.clone());
        let mut events = event_bus.subscribe();
        let (scenes, task, _peers) = build_scenes_actor(
            1,
            RuntimeGeneration::default(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            test_lockout_reader(),
        );
        task.spawn();
        peers.set_scenes(scenes.clone());
        let new_lv1 = lv1_snapshot(vec![SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }]);
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let handle = crate::lv1::test_actor_handle(tx);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if let crate::lv1::Lv1Command::GetState { reply } = command {
                    let _ = reply.send(new_lv1.clone());
                }
            }
        });
        peers.set_lv1(0, handle);

        let path = std::env::temp_dir().join(format!("show-load-{}.ascs", Uuid::new_v4()));
        let file = crate::show::show_file::ShowFile {
            schema_version: crate::show::SHOW_FILE_SCHEMA_VERSION,
            app_version: "test".to_string(),
            saved_at: "123".to_string(),
            safety: crate::show::show_file::ShowFileSafety { lockout: false },
            scene_configs: vec![file_scene(scene_config(1, Some(1), "Intro", 1_000))],
            cue_lists: Vec::new(),
            active_cue_list_id: None,
            cued_cue_entry_id: None,
        };
        crate::show_file::write_show_file(&path, &file, &crate::show_file::backup_folder())
            .unwrap();

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::LoadShowFileFromPath {
            path: path.clone(),
            reply: Some(reply),
        })
        .await
        .unwrap();

        assert!(rx.await.unwrap().is_ok());

        let state = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            recv_file_metadata_event(&mut events),
        )
        .await
        .unwrap();
        assert!(!state.show_file_dirty);
        let scene_document = get_scene_document(&scenes).await;
        assert_eq!(scene_document.scene_configs[0].scene_name, "Intro");
        assert_eq!(
            scene_document.selected_scene_internal_id,
            Some(Uuid::from_u128(1).to_string())
        );
    }

    #[tokio::test]
    async fn load_marks_dirty_when_cue_reconciliation_clears_invalid_active_cue() {
        let event_bus = AppEventBus::default();
        let (show, peers) = show_actor(event_bus.clone());
        let mut events = event_bus.subscribe();
        let (scenes, task, _peers) = build_scenes_actor(
            1,
            RuntimeGeneration::default(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            test_lockout_reader(),
        );
        let cue_lists = task.cue_lists_handle();
        task.spawn();
        peers.set_scenes(scenes.clone());
        let new_lv1 = lv1_snapshot(vec![SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }]);
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let handle = crate::lv1::test_actor_handle(tx);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if let crate::lv1::Lv1Command::GetState { reply } = command {
                    let _ = reply.send(new_lv1.clone());
                }
            }
        });
        peers.set_lv1(0, handle);

        let path = std::env::temp_dir().join(format!("show-load-cue-{}.ascs", Uuid::new_v4()));
        let cue_list_id = Uuid::new_v4();
        let cue_entry_id = Uuid::new_v4();
        let missing_scene_id = Uuid::new_v4();
        let file = crate::show::show_file::ShowFile {
            schema_version: crate::show::SHOW_FILE_SCHEMA_VERSION,
            app_version: "test".to_string(),
            saved_at: "123".to_string(),
            safety: crate::show::show_file::ShowFileSafety { lockout: false },
            scene_configs: vec![file_scene(scene_config(1, Some(1), "Intro", 1_000))],
            cue_lists: vec![crate::cue_lists::CueList {
                id: cue_list_id,
                name: "Main".to_string(),
                entries: vec![crate::cue_lists::CueEntry {
                    id: cue_entry_id,
                    scene_internal_id: missing_scene_id,
                }],
            }],
            active_cue_list_id: Some(cue_list_id),
            cued_cue_entry_id: Some(cue_entry_id),
        };
        crate::show_file::write_show_file(&path, &file, &crate::show_file::backup_folder())
            .unwrap();

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::LoadShowFileFromPath {
            path: path.clone(),
            reply: Some(reply),
        })
        .await
        .unwrap();

        assert!(rx.await.unwrap().is_ok());

        let state = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            recv_file_metadata_event(&mut events),
        )
        .await
        .unwrap();
        assert!(state.show_file_dirty);
        let cue_document = get_cue_list_document(&cue_lists).await;
        assert_eq!(cue_document.cued_cue_entry_id, None);
    }

    #[tokio::test]
    async fn persisted_scene_edits_dirty_the_file_but_projection_only_facts_do_not() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let (_show, task, _, _) = super::build_show_actor(event_bus.clone());
        let scene_event = |persisted_scene_edit| crate::runtime::events::AppEvent::Scenes {
            generation: 1,
            event: crate::scenes::ScenesEvent::StateChanged {
                state: crate::scenes::ScenesProjectionState::default(),
                persisted_scene_edit,
            },
        };

        event_bus.publish(scene_event(true));
        task.spawn();
        let dirty_state = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            recv_file_metadata_event(&mut events),
        )
        .await
        .unwrap();
        assert!(dirty_state.show_file_dirty);

        event_bus.publish(scene_event(false));
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(100),
                recv_file_metadata_event(&mut events),
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn new_show_fails_when_scenes_state_is_unavailable() {
        let event_bus = AppEventBus::default();
        let (show, _peers) = show_actor(event_bus.clone());
        let mut events = event_bus.subscribe();

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::NewShowFileFromCurrentLv1 { reply: Some(reply) })
            .await
            .unwrap();

        assert_eq!(rx.await.unwrap().unwrap_err(), "LV1 actor is unavailable");
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn new_show_replaces_scenes_state_from_current_lv1_and_marks_metadata_clean() {
        let event_bus = AppEventBus::default();
        let (show, peers) = show_actor(event_bus.clone());
        let mut events = event_bus.subscribe();
        let (scenes, task, _peers) = build_scenes_actor(
            1,
            RuntimeGeneration::default(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            test_lockout_reader(),
        );
        task.spawn();
        peers.set_scenes(scenes.clone());
        event_bus.publish(crate::runtime::events::AppEvent::Runtime(
            RuntimeLifecycleEvent::ActiveGenerationChanged { generation: 1 },
        ));
        tokio::task::yield_now().await;

        let new_lv1 = lv1_snapshot(vec![
            SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            },
            SceneListEntry {
                index: 2,
                name: "Verse".to_string(),
            },
        ]);
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let handle = crate::lv1::test_actor_handle(tx);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if let crate::lv1::Lv1Command::GetState { reply } = command {
                    let _ = reply.send(new_lv1.clone());
                }
            }
        });
        peers.set_lv1(0, handle);

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::NewShowFileFromCurrentLv1 { reply: Some(reply) })
            .await
            .unwrap();

        assert!(rx.await.unwrap().is_ok());

        let state = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            recv_file_metadata_event(&mut events),
        )
        .await
        .unwrap();
        assert!(!state.show_file_dirty);
        let scene_document = get_scene_document(&scenes).await;
        assert_eq!(scene_document.scene_configs.len(), 2);
        assert_eq!(scene_document.scene_configs[0].scene_name, "Intro");
    }

    #[tokio::test]
    async fn persisted_cue_list_edit_marks_show_file_dirty() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let (_show, task, _, _) = super::build_show_actor(event_bus.clone());
        event_bus.publish(crate::runtime::events::AppEvent::CueLists(
            CueListsProjectionState::default(),
        ));
        task.spawn();

        let dirty_state = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            recv_file_metadata_event(&mut events),
        )
        .await
        .unwrap();

        assert!(dirty_state.show_file_dirty);
    }
}

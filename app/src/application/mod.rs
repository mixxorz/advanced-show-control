//! Host-neutral application command adapters.
//!
//! These adapters contain no windowing or dialog behavior. Hosts are responsible for collecting
//! file paths before invoking file commands.

use std::path::PathBuf;

use tokio::sync::oneshot;
use uuid::Uuid;

use crate::connection_state::Lv1SystemIdentity;
use crate::cue_lists::{CueListsCommand, CueListsCommandResult, CueListsHandle, CueRecallResult};
use crate::lifecycle::AppLifecycle;
use crate::logging::UiLogEvent;
use crate::lv1::TcpConnectProbeResult;
use crate::projector::ProjectionSubscription;
use crate::runtime::errors::AppCommandError;
use crate::scenes::{
    RecallSceneResult, ScenesCommand, ScenesCommandResult, ScenesHandle, SelectedSceneResult,
};
use crate::settings::{AppSettings, SettingsCommand, SettingsCommandResult, SettingsHandle};
use crate::show::{
    ConnectCommandResult, LoadShowFileResult, NewShowFileResult, ShowCommand, ShowCommandResult,
    ShowStateHandle,
};

/// Cloneable access to the app-lifetime command owners.
///
/// Generation-scoped Fade access remains behind `AppLifecycle`; fade cancellation deliberately
/// routes through Scenes so the existing safety and cancellation policy cannot be bypassed.
#[derive(Clone)]
pub struct ApplicationCommandContext {
    lifecycle: AppLifecycle,
    show: ShowStateHandle,
    settings: SettingsHandle,
    scenes: ScenesHandle,
    cue_lists: CueListsHandle,
    ui_logs: tokio::sync::broadcast::Sender<UiLogEvent>,
}

impl ApplicationCommandContext {
    pub fn new(
        lifecycle: AppLifecycle,
        show: ShowStateHandle,
        settings: SettingsHandle,
        ui_logs: tokio::sync::broadcast::Sender<UiLogEvent>,
    ) -> Self {
        let scenes = lifecycle.scenes_handle();
        let cue_lists = lifecycle.cue_lists_handle();
        Self {
            lifecycle,
            show,
            settings,
            scenes,
            cue_lists,
            ui_logs,
        }
    }

    pub async fn frontend_ready(&self) -> Result<ProjectionSubscription, String> {
        self.lifecycle
            .frontend_ready(self.ui_logs.subscribe())
            .await
    }

    pub async fn connect_lv1_system(
        &self,
        identity: Lv1SystemIdentity,
    ) -> Result<ConnectCommandResult, String> {
        self.lifecycle.connect_lv1_system(identity).await
    }

    pub async fn startup_auto_connect_lv1(&self) -> Result<ConnectCommandResult, String> {
        self.lifecycle.startup_auto_connect_lv1().await
    }

    pub async fn refresh_lv1_discovery(
        &self,
        timeout_ms: Option<u64>,
    ) -> Result<ShowCommandResult, String> {
        self.lifecycle.refresh_lv1_discovery(timeout_ms).await
    }

    pub async fn probe_lv1_tcp_connect_latency(
        &self,
        identity: Lv1SystemIdentity,
        timeout_ms: Option<u64>,
    ) -> Result<TcpConnectProbeResult, String> {
        crate::lv1::probe_tcp_connect_latency(&identity.address, identity.port, timeout_ms).await
    }

    pub async fn disconnect_lv1(&self) -> Result<ShowCommandResult, String> {
        self.lifecycle.disconnect_current_runtime().await
    }

    pub async fn new_show_file(&self) -> Result<NewShowFileResult, String> {
        let (reply, response) = oneshot::channel();
        self.send_show(ShowCommand::NewShowFileFromCurrentLv1 { reply: Some(reply) })
            .await?;
        receive_nested(response).await
    }

    /// @cc [owner:mixxorz,label:product] open-cancellation-is-not-an-error
    /// A missing path means the host picker was cancelled and MUST return `Ok(None)` without
    /// dispatching a Show command or changing the current session.
    pub async fn open_show_file(
        &self,
        path: Option<PathBuf>,
    ) -> Result<Option<LoadShowFileResult>, String> {
        let Some(path) = path else {
            return Ok(None);
        };
        let (reply, response) = oneshot::channel();
        self.send_show(ShowCommand::LoadShowFileFromPath {
            path,
            reply: Some(reply),
        })
        .await?;
        receive_nested(response).await.map(Some)
    }

    /// A missing path means the host picker was cancelled without changing the current session.
    pub async fn new_show_file_from_template(
        &self,
        path: Option<PathBuf>,
    ) -> Result<Option<NewShowFileResult>, String> {
        let Some(path) = path else {
            return Ok(None);
        };
        let (reply, response) = oneshot::channel();
        self.send_show(ShowCommand::NewShowFileFromTemplate {
            path,
            reply: Some(reply),
        })
        .await?;
        receive_nested(response).await.map(Some)
    }

    /// Saves to the current Show-owned path, or to `fallback_path` when the show has no path yet.
    /// A missing current and fallback path means the host picker was cancelled.
    pub async fn save_show_file(
        &self,
        fallback_path: Option<PathBuf>,
    ) -> Result<Option<ShowCommandResult>, String> {
        let (reply, response) = oneshot::channel();
        self.send_show(ShowCommand::CurrentShowFilePath { reply })
            .await?;
        let Some(path) = receive(response).await?.or(fallback_path) else {
            return Ok(None);
        };
        self.save_show_file_to_path(path).await.map(Some)
    }

    /// @cc [owner:mixxorz,label:product] save-as-cancellation-is-not-an-error
    /// A missing path means the host picker was cancelled and MUST return `Ok(None)` without
    /// dispatching a Show save command or changing the current session.
    pub async fn save_show_file_as(
        &self,
        path: Option<PathBuf>,
    ) -> Result<Option<ShowCommandResult>, String> {
        let Some(path) = path else {
            return Ok(None);
        };
        self.save_show_file_to_path(path).await.map(Some)
    }

    async fn save_show_file_to_path(&self, path: PathBuf) -> Result<ShowCommandResult, String> {
        let (reply, response) = oneshot::channel();
        self.send_show(ShowCommand::SaveShowFileAs {
            path,
            reply: Some(reply),
        })
        .await?;
        receive_nested(response).await
    }

    pub async fn set_lockout(&self, enabled: bool) -> Result<ShowCommandResult, String> {
        let (reply, response) = oneshot::channel();
        self.send_show(ShowCommand::SetLockout {
            enabled,
            reply: Some(reply),
        })
        .await?;
        receive(response).await
    }

    pub async fn replace_app_settings(
        &self,
        settings: AppSettings,
    ) -> Result<SettingsCommandResult, String> {
        let (reply, response) = oneshot::channel();
        self.settings
            .send(SettingsCommand::ReplaceSettings { settings, reply })
            .await
            .map_err(|_| {
                map_app_command_error(AppCommandError::CommandFailed(
                    "Settings unavailable".to_string(),
                ))
            })?;
        receive_nested(response).await
    }

    pub async fn recall_scene(&self, internal_scene_id: Uuid) -> Result<RecallSceneResult, String> {
        self.send_scene(|reply| ScenesCommand::RecallScene {
            internal_scene_id,
            reply,
        })
        .await?
        .map_err(map_app_command_error)
    }

    pub async fn set_scene_duration_ms(
        &self,
        internal_scene_id: Uuid,
        duration_ms: u64,
    ) -> Result<ScenesCommandResult, String> {
        self.send_scene_mutation(|reply| ScenesCommand::SetSceneDuration {
            internal_scene_id,
            duration_ms,
            reply: Some(reply),
        })
        .await
    }

    pub async fn link_scene_config(
        &self,
        source_internal_scene_id: Uuid,
        target_scene_index: i32,
        overwrite_existing: bool,
    ) -> Result<ScenesCommandResult, String> {
        self.send_scene_mutation(|reply| ScenesCommand::LinkSceneConfig {
            source_internal_scene_id,
            target_scene_index,
            overwrite_existing,
            reply: Some(reply),
        })
        .await
    }

    pub async fn delete_scene_config(
        &self,
        internal_scene_id: Uuid,
    ) -> Result<ScenesCommandResult, String> {
        self.send_scene_mutation(|reply| ScenesCommand::DeleteSceneConfig {
            internal_scene_id,
            reply: Some(reply),
        })
        .await
    }

    pub async fn copy_scene_settings(
        &self,
        internal_scene_id: Uuid,
    ) -> Result<ScenesCommandResult, String> {
        self.send_scene_mutation(|reply| ScenesCommand::CopySceneSettings {
            source_internal_scene_id: internal_scene_id,
            reply: Some(reply),
        })
        .await
    }

    pub async fn paste_scene_settings(
        &self,
        internal_scene_id: Uuid,
    ) -> Result<ScenesCommandResult, String> {
        self.send_scene_mutation(|reply| ScenesCommand::PasteSceneSettings {
            destination_internal_scene_id: internal_scene_id,
            reply: Some(reply),
        })
        .await
    }

    pub async fn select_scene_config(
        &self,
        internal_scene_id: Uuid,
    ) -> Result<SelectedSceneResult, String> {
        self.send_scene_mutation(|reply| ScenesCommand::SelectSceneConfig {
            internal_scene_id,
            reply: Some(reply),
        })
        .await
    }

    pub async fn store_scene_config(
        &self,
        internal_scene_id: Uuid,
    ) -> Result<ScenesCommandResult, String> {
        self.send_scene_mutation(|reply| ScenesCommand::StoreSceneConfigFromCurrentLv1 {
            internal_scene_id,
            reply: Some(reply),
        })
        .await
    }

    pub async fn set_all_channels_scoped(
        &self,
        internal_scene_id: Uuid,
        scoped: bool,
    ) -> Result<ScenesCommandResult, String> {
        self.send_scene_mutation(|reply| ScenesCommand::SetAllChannelsScoped {
            internal_scene_id,
            scoped,
            reply: Some(reply),
        })
        .await
    }

    pub async fn set_scene_scope_faders_enabled(
        &self,
        internal_scene_id: Uuid,
        enabled: bool,
    ) -> Result<ScenesCommandResult, String> {
        self.send_scene_mutation(|reply| ScenesCommand::SetSceneScopeFadersEnabled {
            internal_scene_id,
            enabled,
            reply: Some(reply),
        })
        .await
    }

    pub async fn set_scene_scope_pan_enabled(
        &self,
        internal_scene_id: Uuid,
        enabled: bool,
    ) -> Result<ScenesCommandResult, String> {
        self.send_scene_mutation(|reply| ScenesCommand::SetSceneScopePanEnabled {
            internal_scene_id,
            enabled,
            reply: Some(reply),
        })
        .await
    }

    pub async fn set_channel_scoped(
        &self,
        internal_scene_id: Uuid,
        group: i32,
        channel: i32,
        scoped: bool,
    ) -> Result<ScenesCommandResult, String> {
        self.send_scene_mutation(|reply| ScenesCommand::SetChannelScoped {
            internal_scene_id,
            group,
            channel,
            scoped,
            reply: Some(reply),
        })
        .await
    }

    pub async fn abort_all_fades(&self) -> Result<(), String> {
        let result = self
            .send_scene(|reply| ScenesCommand::AbortAll { reply })
            .await?;
        result.map_err(map_app_command_error)
    }

    pub async fn create_cue_list(&self, name: String) -> Result<CueListsCommandResult, String> {
        self.send_cue_mutation(|reply| CueListsCommand::CreateCueList {
            name,
            reply: Some(reply),
        })
        .await
    }

    pub async fn rename_cue_list(
        &self,
        cue_list_id: Uuid,
        name: String,
    ) -> Result<CueListsCommandResult, String> {
        self.send_cue_mutation(|reply| CueListsCommand::RenameCueList {
            cue_list_id,
            name,
            reply: Some(reply),
        })
        .await
    }

    pub async fn delete_cue_list(
        &self,
        cue_list_id: Uuid,
    ) -> Result<CueListsCommandResult, String> {
        self.send_cue_mutation(|reply| CueListsCommand::DeleteCueList {
            cue_list_id,
            reply: Some(reply),
        })
        .await
    }

    pub async fn reorder_cue_lists(
        &self,
        ordered_ids: Vec<Uuid>,
    ) -> Result<CueListsCommandResult, String> {
        self.send_cue_mutation(|reply| CueListsCommand::ReorderCueLists {
            ordered_ids,
            reply: Some(reply),
        })
        .await
    }

    pub async fn set_active_cue_list(
        &self,
        cue_list_id: Option<Uuid>,
    ) -> Result<CueListsCommandResult, String> {
        self.send_cue_mutation(|reply| CueListsCommand::SetActiveCueList {
            cue_list_id,
            reply: Some(reply),
        })
        .await
    }

    pub async fn add_scene_to_active_cue_list(
        &self,
        scene_internal_id: Uuid,
        insert_index: usize,
    ) -> Result<CueListsCommandResult, String> {
        self.send_cue_mutation(|reply| CueListsCommand::AddSceneToActiveCueList {
            scene_internal_id,
            insert_index,
            reply: Some(reply),
        })
        .await
    }

    pub async fn remove_cue_entry(
        &self,
        cue_entry_id: Uuid,
    ) -> Result<CueListsCommandResult, String> {
        self.send_cue_mutation(|reply| CueListsCommand::RemoveCueEntry {
            cue_entry_id,
            reply: Some(reply),
        })
        .await
    }

    pub async fn reorder_cue_entries(
        &self,
        ordered_entry_ids: Vec<Uuid>,
    ) -> Result<CueListsCommandResult, String> {
        self.send_cue_mutation(|reply| CueListsCommand::ReorderCueEntries {
            ordered_entry_ids,
            reply: Some(reply),
        })
        .await
    }

    pub async fn cue_entry(
        &self,
        cue_entry_id: Option<Uuid>,
    ) -> Result<CueListsCommandResult, String> {
        self.send_cue_mutation(|reply| CueListsCommand::CueEntry {
            cue_entry_id,
            reply: Some(reply),
        })
        .await
    }

    pub async fn recall_cued_cue(&self) -> Result<CueRecallResult, String> {
        let (reply, response) = oneshot::channel();
        self.cue_lists
            .send(CueListsCommand::RecallCuedCue { reply })
            .await
            .map_err(|_| cue_lists_unavailable())?;
        receive(response).await?.map_err(map_app_command_error)
    }

    async fn send_show(&self, command: ShowCommand) -> Result<(), String> {
        self.show
            .send(command)
            .await
            .map_err(|_| map_app_command_error(AppCommandError::ShowUnavailable))
    }

    async fn send_scene<T>(
        &self,
        build_command: impl FnOnce(oneshot::Sender<T>) -> ScenesCommand,
    ) -> Result<T, String> {
        let (reply, response) = oneshot::channel();
        self.scenes
            .send(build_command(reply))
            .await
            .map_err(|_| map_app_command_error(AppCommandError::ScenesUnavailable))?;
        receive(response).await
    }

    async fn send_scene_mutation<T>(
        &self,
        build_command: impl FnOnce(oneshot::Sender<Result<T, String>>) -> ScenesCommand,
    ) -> Result<T, String> {
        self.send_scene(build_command).await?
    }

    async fn send_cue_mutation(
        &self,
        build_command: impl FnOnce(
            oneshot::Sender<Result<CueListsCommandResult, String>>,
        ) -> CueListsCommand,
    ) -> Result<CueListsCommandResult, String> {
        let (reply, response) = oneshot::channel();
        self.cue_lists
            .send(build_command(reply))
            .await
            .map_err(|_| cue_lists_unavailable())?;
        receive_nested(response).await
    }
}

fn cue_lists_unavailable() -> String {
    map_app_command_error(AppCommandError::CommandFailed(
        "cue lists are unavailable".to_string(),
    ))
}

fn map_app_command_error(error: AppCommandError) -> String {
    match error {
        AppCommandError::CommandFailed(message) => message,
        other => other.to_string(),
    }
}

async fn receive<T>(response: oneshot::Receiver<T>) -> Result<T, String> {
    response
        .await
        .map_err(|_| map_app_command_error(AppCommandError::ReplyChannelClosed))
}

async fn receive_nested<T>(response: oneshot::Receiver<Result<T, String>>) -> Result<T, String> {
    receive(response).await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::events::AppEventBus;

    fn context_with_show(show: ShowStateHandle) -> ApplicationCommandContext {
        let event_bus = AppEventBus::default();
        let (_owned_show, show_task, show_peers, lockout) =
            crate::show::build_show_actor(event_bus.clone());
        let settings_dir =
            std::env::temp_dir().join(format!("asc-application-test-{}", Uuid::new_v4()));
        let (settings, settings_task, initial_settings) =
            crate::settings::build_settings_actor(settings_dir, event_bus.clone());
        let lifecycle = AppLifecycle::new(
            event_bus,
            show.clone(),
            show_peers,
            lockout,
            settings.clone(),
            initial_settings,
        );
        drop(show_task);
        settings_task.spawn();
        let (ui_logs, _) = tokio::sync::broadcast::channel(8);
        ApplicationCommandContext::new(lifecycle, show, settings, ui_logs)
    }

    #[tokio::test]
    async fn cancelled_file_operations_do_not_dispatch_show_commands() {
        let (show, mut commands) = tokio::sync::mpsc::channel(4);
        let context = context_with_show(show);

        assert_eq!(context.open_show_file(None).await.unwrap(), None);
        assert_eq!(
            context.new_show_file_from_template(None).await.unwrap(),
            None
        );
        assert_eq!(context.save_show_file_as(None).await.unwrap(), None);
        assert!(commands.try_recv().is_err());
    }

    #[tokio::test]
    async fn new_from_template_dispatches_the_selected_path() {
        let template = PathBuf::from("template.ascs");
        let expected = template.clone();
        let (show, mut commands) = tokio::sync::mpsc::channel(4);
        let context = context_with_show(show);
        let actor = tokio::spawn(async move {
            let ShowCommand::NewShowFileFromTemplate { path, reply } =
                commands.recv().await.unwrap()
            else {
                panic!("expected new-from-template command");
            };
            assert_eq!(path, expected);
            reply
                .unwrap()
                .send(Ok(NewShowFileResult {
                    selected_scene_internal_id: Some("scene-id".to_string()),
                }))
                .unwrap();
        });

        assert_eq!(
            context
                .new_show_file_from_template(Some(template))
                .await
                .unwrap()
                .unwrap()
                .selected_scene_internal_id,
            Some("scene-id".to_string())
        );
        actor.await.unwrap();
    }

    #[tokio::test]
    async fn save_without_an_authoritative_current_path_requests_a_destination() {
        let (show, mut commands) = tokio::sync::mpsc::channel(4);
        let context = context_with_show(show);
        let actor = tokio::spawn(async move {
            let ShowCommand::CurrentShowFilePath { reply } = commands.recv().await.unwrap() else {
                panic!("expected current-path query");
            };
            reply.send(None).unwrap();
            assert!(commands.try_recv().is_err());
        });

        assert_eq!(context.save_show_file(None).await.unwrap(), None);
        actor.await.unwrap();
    }

    #[tokio::test]
    async fn save_prefers_the_show_owned_current_path_over_the_fallback() {
        let current = PathBuf::from("current.ascs");
        let fallback = PathBuf::from("fallback.ascs");
        let (show, mut commands) = tokio::sync::mpsc::channel(4);
        let context = context_with_show(show);
        let expected = current.clone();
        let actor = tokio::spawn(async move {
            let ShowCommand::CurrentShowFilePath { reply } = commands.recv().await.unwrap() else {
                panic!("expected current-path query");
            };
            reply.send(Some(current)).unwrap();
            let ShowCommand::SaveShowFileAs { path, reply } = commands.recv().await.unwrap() else {
                panic!("expected save command");
            };
            assert_eq!(path, expected);
            reply
                .unwrap()
                .send(Ok(ShowCommandResult { changed: true }))
                .unwrap();
        });

        assert!(
            context
                .save_show_file(Some(fallback))
                .await
                .unwrap()
                .expect("save should run")
                .changed
        );
        actor.await.unwrap();
    }

    #[test]
    fn command_failed_preserves_its_frontend_safe_message() {
        assert_eq!(
            map_app_command_error(AppCommandError::CommandFailed(
                "specific failure".to_string()
            )),
            "specific failure"
        );
        assert_eq!(
            map_app_command_error(AppCommandError::ScenesUnavailable),
            "scene state is unavailable"
        );
    }
}

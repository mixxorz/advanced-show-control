use tokio::sync::{mpsc, oneshot};

use crate::lv1::types::{ChannelInfo, SceneListEntry};

use super::commands::ShowCommand;
use super::types::{SceneConfig, ShowSnapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShowActorError {
    CommandChannelClosed,
    ReplyChannelClosed,
}

#[derive(Clone)]
pub struct ShowStateHandle {
    tx: mpsc::Sender<ShowCommand>,
}

impl ShowStateHandle {
    pub(crate) fn new(tx: mpsc::Sender<ShowCommand>) -> Self {
        Self { tx }
    }

    async fn request<T>(
        &self,
        command: ShowCommand,
        reply_rx: oneshot::Receiver<T>,
    ) -> Result<T, ShowActorError> {
        self.tx
            .send(command)
            .await
            .map_err(|_| ShowActorError::CommandChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| ShowActorError::ReplyChannelClosed)
    }

    pub async fn get_snapshot(&self) -> Result<ShowSnapshot, ShowActorError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request(ShowCommand::GetSnapshot { reply }, reply_rx)
            .await
    }

    pub async fn get_scene_config(
        &self,
        scene_id: String,
    ) -> Result<Option<SceneConfig>, ShowActorError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request(ShowCommand::GetSceneConfig { scene_id, reply }, reply_rx)
            .await
    }

    pub async fn get_lockout(&self) -> Result<bool, ShowActorError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request(ShowCommand::GetLockout { reply }, reply_rx)
            .await
    }

    pub async fn set_lockout(&self, enabled: bool) -> Result<bool, ShowActorError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request(ShowCommand::SetLockout { enabled, reply }, reply_rx)
            .await
    }

    pub async fn set_scene_duration(
        &self,
        scene_id: String,
        duration_ms: u64,
    ) -> Result<Result<bool, String>, ShowActorError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request(
            ShowCommand::SetSceneDuration {
                scene_id,
                duration_ms,
                reply,
            },
            reply_rx,
        )
        .await
    }

    pub async fn set_scene_scope_faders_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<Result<bool, String>, ShowActorError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request(
            ShowCommand::SetSceneScopeFadersEnabled {
                scene_id,
                enabled,
                reply,
            },
            reply_rx,
        )
        .await
    }

    pub async fn set_channel_scoped(
        &self,
        scene_id: String,
        group: i32,
        channel: i32,
        scoped: bool,
    ) -> Result<Result<bool, String>, ShowActorError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request(
            ShowCommand::SetChannelScoped {
                scene_id,
                group,
                channel,
                scoped,
                reply,
            },
            reply_rx,
        )
        .await
    }

    pub async fn set_all_channels_scoped(
        &self,
        scene_id: String,
        scoped: bool,
    ) -> Result<Result<bool, String>, ShowActorError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request(
            ShowCommand::SetAllChannelsScoped {
                scene_id,
                scoped,
                reply,
            },
            reply_rx,
        )
        .await
    }

    pub async fn store_scene_config(
        &self,
        scene_id: String,
        channels: Vec<ChannelInfo>,
    ) -> Result<Result<bool, String>, ShowActorError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request(
            ShowCommand::StoreSceneConfig {
                scene_id,
                channels,
                reply,
            },
            reply_rx,
        )
        .await
    }

    pub async fn reconcile_scene_list(
        &self,
        scenes: Vec<SceneListEntry>,
    ) -> Result<bool, ShowActorError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request(ShowCommand::ReconcileSceneList { scenes, reply }, reply_rx)
            .await
    }

    pub async fn replace_snapshot(&self, snapshot: ShowSnapshot) -> Result<(), ShowActorError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request(ShowCommand::ReplaceSnapshot { snapshot, reply }, reply_rx)
            .await
    }

    pub async fn clear(&self) -> Result<(), ShowActorError> {
        let (reply, reply_rx) = oneshot::channel();
        self.request(ShowCommand::Clear { reply }, reply_rx).await
    }
}

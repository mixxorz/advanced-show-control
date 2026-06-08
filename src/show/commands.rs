use tokio::sync::oneshot;

use crate::lv1::types::SceneListEntry;

use super::types::{SceneConfig, ShowSnapshot};

pub enum ShowCommand {
    GetSnapshot { reply: oneshot::Sender<ShowSnapshot> },
    GetSceneConfig { scene_id: String, reply: oneshot::Sender<Option<SceneConfig>> },
    GetLockout { reply: oneshot::Sender<bool> },
    SetLockout { enabled: bool, reply: oneshot::Sender<bool> },
    SetSceneDuration { scene_id: String, duration_ms: u64, reply: oneshot::Sender<Result<bool, String>> },
    SetChannelScoped { scene_id: String, group: i32, channel: i32, scoped: bool, reply: oneshot::Sender<Result<bool, String>> },
    SetAllChannelsScoped { scene_id: String, scoped: bool, reply: oneshot::Sender<Result<bool, String>> },
    StoreSceneConfig { scene_id: String, channels: Vec<crate::lv1::types::ChannelInfo>, reply: oneshot::Sender<Result<bool, String>> },
    ReconcileSceneList { scenes: Vec<SceneListEntry>, reply: oneshot::Sender<bool> },
    ReplaceSnapshot { snapshot: ShowSnapshot, reply: oneshot::Sender<()> },
    Clear { reply: oneshot::Sender<()> },
}

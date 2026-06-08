use tokio::sync::{mpsc, oneshot};

use crate::lv1::types::{ChannelInfo, SceneListEntry};

use super::commands::ShowCommand;
use super::types::{SceneConfig, ShowSnapshot};

#[derive(Clone)]
pub struct ShowStateHandle { tx: mpsc::Sender<ShowCommand> }

impl ShowStateHandle {
    pub(crate) fn new(tx: mpsc::Sender<ShowCommand>) -> Self { Self { tx } }
    pub async fn get_snapshot(&self) -> ShowSnapshot { let (reply, rx)=oneshot::channel(); let _=self.tx.send(ShowCommand::GetSnapshot{reply}).await; rx.await.expect("actor dropped") }
    pub async fn get_scene_config(&self, scene_id: String) -> Option<SceneConfig> { let (reply, rx)=oneshot::channel(); let _=self.tx.send(ShowCommand::GetSceneConfig{scene_id, reply}).await; rx.await.expect("actor dropped") }
    pub async fn get_lockout(&self) -> bool { let (reply, rx)=oneshot::channel(); let _=self.tx.send(ShowCommand::GetLockout{reply}).await; rx.await.expect("actor dropped") }
    pub async fn set_lockout(&self, enabled: bool) -> bool { let (reply, rx)=oneshot::channel(); let _=self.tx.send(ShowCommand::SetLockout{enabled, reply}).await; rx.await.expect("actor dropped") }
    pub async fn set_scene_duration(&self, scene_id: String, duration_ms: u64) -> Result<bool, String> { let (reply, rx)=oneshot::channel(); self.tx.send(ShowCommand::SetSceneDuration{scene_id, duration_ms, reply}).await.unwrap(); rx.await.expect("actor dropped") }
    pub async fn set_channel_scoped(&self, scene_id: String, group: i32, channel: i32, scoped: bool) -> Result<bool, String> { let (reply, rx)=oneshot::channel(); self.tx.send(ShowCommand::SetChannelScoped{scene_id, group, channel, scoped, reply}).await.unwrap(); rx.await.expect("actor dropped") }
    pub async fn set_all_channels_scoped(&self, scene_id: String, scoped: bool) -> Result<bool, String> { let (reply, rx)=oneshot::channel(); self.tx.send(ShowCommand::SetAllChannelsScoped{scene_id, scoped, reply}).await.unwrap(); rx.await.expect("actor dropped") }
    pub async fn store_scene_config(&self, scene_id: String, channels: Vec<ChannelInfo>) -> Result<bool, String> { let (reply, rx)=oneshot::channel(); self.tx.send(ShowCommand::StoreSceneConfig{scene_id, channels, reply}).await.unwrap(); rx.await.expect("actor dropped") }
    pub async fn load_show_data(&self) -> Result<(), String> { let (reply, rx)=oneshot::channel(); self.tx.send(ShowCommand::LoadShowData{reply}).await.unwrap(); rx.await.expect("actor dropped") }
    pub async fn export_show_data(&self) -> Result<(), String> { let (reply, rx)=oneshot::channel(); self.tx.send(ShowCommand::ExportShowData{reply}).await.unwrap(); rx.await.expect("actor dropped") }
    pub async fn reconcile_scene_list(&self, scenes: Vec<SceneListEntry>) -> bool { let (reply, rx)=oneshot::channel(); self.tx.send(ShowCommand::ReconcileSceneList{scenes, reply}).await.unwrap(); rx.await.expect("actor dropped") }
}

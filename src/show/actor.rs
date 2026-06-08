use tokio::sync::mpsc;

use crate::runtime::events::{AppEvent, AppEventBus};

use super::commands::ShowCommand;
use super::events::ShowEvent;
use super::handle::ShowStateHandle;
use super::state::ShowState;

pub fn spawn_show_state(event_bus: AppEventBus) -> ShowStateHandle {
    let (tx, mut rx) = mpsc::channel(32);
    let handle = ShowStateHandle::new(tx);
    tokio::spawn(async move {
        let mut state = ShowState { lockout: false, scene_configs: Vec::new() };
        while let Some(cmd) = rx.recv().await {
            match cmd {
                ShowCommand::GetSnapshot { reply } => { let _ = reply.send(state.snapshot()); }
                ShowCommand::GetSceneConfig { scene_id, reply } => { let _ = reply.send(state.get_scene_config(&scene_id)); }
                ShowCommand::GetLockout { reply } => { let _ = reply.send(state.lockout); }
                ShowCommand::SetLockout { enabled, reply } => { let changed = state.set_lockout(enabled); let _ = reply.send(changed); if changed { event_bus.publish(AppEvent::Show(ShowEvent::LockoutChanged { enabled })); } }
                ShowCommand::SetSceneDuration { scene_id, duration_ms, reply } => { let result = state.set_scene_duration_ms(&scene_id, duration_ms); if matches!(result, Ok(true)) { event_bus.publish(AppEvent::Show(ShowEvent::SceneConfigChanged { scene_id })); } let _ = reply.send(result); }
                ShowCommand::SetChannelScoped { scene_id, group, channel, scoped, reply } => { let result = state.set_channel_scoped(&scene_id, group, channel, scoped); if matches!(result, Ok(true)) { event_bus.publish(AppEvent::Show(ShowEvent::SceneConfigChanged { scene_id })); } let _ = reply.send(result); }
                ShowCommand::SetAllChannelsScoped { scene_id, scoped, reply } => { let result = state.set_all_channels_scoped(&scene_id, scoped); if matches!(result, Ok(true)) { event_bus.publish(AppEvent::Show(ShowEvent::SceneConfigChanged { scene_id })); } let _ = reply.send(result); }
                ShowCommand::StoreSceneConfig { scene_id, channels, reply } => { let result = state.store_scene_config(&scene_id, &channels); if matches!(result, Ok(true)) { event_bus.publish(AppEvent::Show(ShowEvent::SceneConfigChanged { scene_id })); } let _ = reply.send(result); }
                ShowCommand::LoadShowData { reply } => { let _ = reply.send(Ok(())); }
                ShowCommand::ExportShowData { reply } => { let _ = reply.send(Ok(())); }
                ShowCommand::ReconcileSceneList { scenes, reply } => { let changed = state.reconcile_scene_fade_configs(&scenes); let _ = reply.send(changed); if changed { event_bus.publish(AppEvent::Show(ShowEvent::StateChanged)); } }
            }
        }
    });
    handle
}

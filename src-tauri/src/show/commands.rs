//! Show-owned application command handlers.

use crate::lv1::types::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot};
use crate::show::show_file::{LoadValidationReport, ShowFile, export_show_file, import_show_file};

use super::handle::ShowStateHandle;
use super::types::SceneConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowCommandResult {
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CueSceneResult {
    pub changed: bool,
    pub scene: SceneConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectedSceneResult {
    pub scene: SceneConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewShowFileResult {
    pub selected_scene_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadShowFileResult {
    pub selected_scene_id: Option<String>,
    pub saved_at: String,
    pub report: LoadValidationReport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecallSceneResult {
    pub scene: SceneConfig,
    pub lv1_scene_index: i32,
}

pub async fn set_lockout(show: &ShowStateHandle, enabled: bool) -> ShowCommandResult {
    ShowCommandResult {
        changed: show.set_lockout(enabled).await,
    }
}

pub async fn set_scene_duration_ms(
    show: &ShowStateHandle,
    scene_id: String,
    duration_ms: u64,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show
            .mutate_for_command(
                super::events::ShowProjectionReason::ShowState,
                move |state| {
                    let changed = state.set_scene_duration_ms(&scene_id, duration_ms)?;
                    if changed {
                        state.mark_dirty();
                    }
                    Ok::<(bool, bool), String>((changed, changed))
                },
            )
            .await?,
    })
}

pub async fn set_scene_scope_faders_enabled(
    show: &ShowStateHandle,
    scene_id: String,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show
            .set_scene_scope_faders_enabled(scene_id, enabled)
            .await?,
    })
}

pub async fn set_scene_scope_pan_enabled(
    show: &ShowStateHandle,
    scene_id: String,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show.set_scene_scope_pan_enabled(scene_id, enabled).await?,
    })
}

pub async fn set_channel_scoped(
    show: &ShowStateHandle,
    scene_id: String,
    group: i32,
    channel: i32,
    scoped: bool,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show
            .set_channel_scoped(scene_id, group, channel, scoped)
            .await?,
    })
}

pub async fn set_all_channels_scoped(
    show: &ShowStateHandle,
    scene_id: String,
    scoped: bool,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show.set_all_channels_scoped(scene_id, scoped).await?,
    })
}

pub async fn cue_scene(show: &ShowStateHandle, scene_id: String) -> Result<CueSceneResult, String> {
    let scene = show
        .get_scene_config(scene_id.clone())
        .await
        .ok_or_else(|| "Scene config not found".to_string())?;
    Ok(CueSceneResult {
        changed: show.cue_scene(scene_id).await?,
        scene,
    })
}

pub async fn select_scene_config(
    show: &ShowStateHandle,
    scene_id: String,
) -> Result<SelectedSceneResult, String> {
    let scene = show
        .get_scene_config(scene_id)
        .await
        .ok_or_else(|| "Scene config not found".to_string())?;
    Ok(SelectedSceneResult { scene })
}

pub fn validate_recall_scene_request(
    show: &super::types::ShowSnapshot,
    lv1: &Lv1StateSnapshot,
    scene_id: &str,
) -> Result<RecallSceneResult, String> {
    if show.lockout {
        return Err("Recall blocked: lockout is enabled".to_string());
    }

    let scene = show
        .scene_configs
        .iter()
        .find(|scene| scene.scene_id == scene_id)
        .cloned()
        .ok_or_else(|| "Scene config not found".to_string())?;

    if lv1.connection != ConnectionStatus::Connected {
        return Err("Recall blocked: LV1 is disconnected".to_string());
    }

    let lv1_scene = lv1
        .scene_list
        .iter()
        .find(|candidate| {
            candidate.index == scene.scene_index && candidate.name == scene.scene_name
        })
        .ok_or_else(|| "Recall blocked: scene identity mismatch".to_string())?;

    Ok(RecallSceneResult {
        scene,
        lv1_scene_index: lv1_scene.index,
    })
}

pub async fn store_scene_config(
    show: &ShowStateHandle,
    scene_id: String,
    channels: Vec<ChannelInfo>,
) -> Result<ShowCommandResult, String> {
    if show.get_scene_config(scene_id.clone()).await.is_none() {
        return Err("Scene config not found".to_string());
    }

    Ok(ShowCommandResult {
        changed: show.store_scene_config(scene_id, channels).await?,
    })
}

pub async fn new_show_file(
    show: &ShowStateHandle,
    lv1: Option<crate::lv1::types::Lv1StateSnapshot>,
) -> Result<NewShowFileResult, String> {
    let selected_scene_id = show
        .mutate_for_command(super::events::ShowProjectionReason::FileMetadata, |state| {
            let selected_scene_id = state.reset_for_new_show(lv1.as_ref());
            Ok::<(bool, Option<String>), String>((true, selected_scene_id))
        })
        .await?;

    Ok(NewShowFileResult { selected_scene_id })
}

pub async fn export_show_file_for_save(show: &ShowStateHandle, saved_at: String) -> ShowFile {
    export_show_file(show.get_snapshot().await, saved_at)
}

pub async fn load_show_file_from_dto(
    show: &ShowStateHandle,
    path: std::path::PathBuf,
    mut file: ShowFile,
    lv1: Option<crate::lv1::types::Lv1StateSnapshot>,
) -> Result<LoadShowFileResult, String> {
    let lv1 = lv1.ok_or_else(|| "Open a show file after LV1 scenes are loaded".to_string())?;
    let imported = import_show_file(&mut file, &lv1)?;
    let saved_at = file.saved_at.clone();
    let selected_scene_id = imported.selected_scene_id.clone();
    let report = imported.report.clone();
    let should_mark_dirty = report.removed_anything();
    let snapshot = imported.snapshot;
    let saved_at_for_state = saved_at.clone();
    let selected_scene_id_for_state = selected_scene_id.clone();
    let report_for_result = report.clone();

    show.mutate_for_command(
        super::events::ShowProjectionReason::FileMetadata,
        move |state| {
            state.replace_snapshot(snapshot);
            state.set_selected_scene_id(selected_scene_id_for_state.clone());
            state.mark_saved(path, saved_at_for_state);
            if should_mark_dirty {
                state.mark_dirty();
            }
            Ok::<(bool, ()), String>((true, ()))
        },
    )
    .await?;

    Ok(LoadShowFileResult {
        selected_scene_id,
        saved_at,
        report: report_for_result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::types::{ConnectionStatus, Lv1StateSnapshot, SceneListEntry};
    use crate::runtime::events::{AppEvent, AppEventBus};
    use crate::show::events::{ShowEvent, ShowProjectionReason};
    use crate::show::handle::ShowStateHandle;
    use crate::show::show_file::{ShowFile, ShowFileSafety, ShowFileSceneConfig};
    use crate::show::types::{SceneConfig, SceneScopeToggles, ShowSnapshot};

    fn recall_lv1(connection: ConnectionStatus, name: &str) -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection,
            scene: None,
            scene_list: vec![SceneListEntry {
                index: 1,
                name: name.to_string(),
            }],
            channels: Vec::new(),
        }
    }

    fn show_file_with_scenes(scene_names: &[&str], cued_scene_id: Option<&str>) -> ShowFile {
        ShowFile {
            schema_version: crate::show::show_file::SHOW_FILE_SCHEMA_VERSION,
            app_version: "test".to_string(),
            saved_at: "saved".to_string(),
            safety: ShowFileSafety { lockout: false },
            scene_configs: scene_names
                .iter()
                .enumerate()
                .map(|(index, name)| ShowFileSceneConfig {
                    scene_index: index as i32 + 1,
                    scene_name: (*name).to_string(),
                    duration_ms: 1_000,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: Default::default(),
                })
                .collect(),
            cued_scene_id: cued_scene_id.map(str::to_string),
        }
    }

    async fn drain_show_events(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
        while events.try_recv().is_ok() {}
    }

    fn recall_show(lockout: bool) -> ShowSnapshot {
        ShowSnapshot {
            lockout,
            scene_configs: vec![SceneConfig {
                scene_id: "1::Verse".to_string(),
                scene_index: 1,
                scene_name: "Verse".to_string(),
                duration_ms: 0,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: SceneScopeToggles::default(),
            }],
            cued_scene_id: Some("1::Verse".to_string()),
        }
    }

    #[test]
    fn validate_recall_scene_request_blocks_lockout_before_lv1_identity() {
        let show = recall_show(true);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Different");

        let err = validate_recall_scene_request(&show, &lv1, "1::Verse").unwrap_err();

        assert_eq!(err, "Recall blocked: lockout is enabled");
    }

    #[test]
    fn validate_recall_scene_request_blocks_missing_scene_config() {
        let show = recall_show(false);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Verse");

        let err = validate_recall_scene_request(&show, &lv1, "2::Chorus").unwrap_err();

        assert_eq!(err, "Scene config not found");
    }

    #[test]
    fn validate_recall_scene_request_blocks_disconnected_lv1() {
        let show = recall_show(false);
        let lv1 = recall_lv1(ConnectionStatus::Disconnected, "Verse");

        let err = validate_recall_scene_request(&show, &lv1, "1::Verse").unwrap_err();

        assert_eq!(err, "Recall blocked: LV1 is disconnected");
    }

    #[test]
    fn validate_recall_scene_request_blocks_scene_identity_mismatch() {
        let show = recall_show(false);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Different");

        let err = validate_recall_scene_request(&show, &lv1, "1::Verse").unwrap_err();

        assert_eq!(err, "Recall blocked: scene identity mismatch");
    }

    #[test]
    fn validate_recall_scene_request_returns_matching_lv1_scene_index() {
        let show = recall_show(false);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Verse");

        let result = validate_recall_scene_request(&show, &lv1, "1::Verse").unwrap();

        assert_eq!(result.scene.scene_id, "1::Verse");
        assert_eq!(result.lv1_scene_index, 1);
    }

    #[tokio::test]
    async fn new_show_file_updates_metadata_and_publishes_state() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Intro");

        let result = new_show_file(&show, Some(lv1)).await.unwrap();

        assert_eq!(result.selected_scene_id, Some("1::Intro".to_string()));
        let event = events.recv().await.unwrap();
        match event {
            AppEvent::Show(ShowEvent::StateChanged { reason, state }) => {
                assert_eq!(reason, ShowProjectionReason::FileMetadata);
                assert_eq!(state.selected_scene_id, Some("1::Intro".to_string()));
                assert!(!state.show_file_dirty);
                assert_eq!(state.show_file_name, "Untitled Show");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn changed_scene_duration_marks_show_dirty_in_command() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);
        new_show_file(
            &show,
            Some(recall_lv1(ConnectionStatus::Connected, "Intro")),
        )
        .await
        .unwrap();
        drain_show_events(&mut events).await;

        let result = set_scene_duration_ms(&show, "1::Intro".to_string(), 1500)
            .await
            .unwrap();

        assert!(result.changed);
        let event = events.recv().await.unwrap();
        match event {
            AppEvent::Show(ShowEvent::StateChanged { state, .. }) => assert!(state.show_file_dirty),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn load_show_file_sets_metadata_restores_selection_and_publishes_once() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Verse");
        let file = show_file_with_scenes(&["Verse"], None);

        let result = load_show_file_from_dto(
            &show,
            std::path::PathBuf::from("/tmp/session.lv1show"),
            file,
            Some(lv1),
        )
        .await
        .unwrap();

        assert_eq!(result.selected_scene_id, Some("1::Verse".to_string()));
        let event = events.recv().await.unwrap();
        match event {
            AppEvent::Show(ShowEvent::StateChanged { reason, state }) => {
                assert_eq!(reason, ShowProjectionReason::FileMetadata);
                assert_eq!(
                    state.show_file_path,
                    Some(std::path::PathBuf::from("/tmp/session.lv1show"))
                );
                assert_eq!(state.show_file_name, "session.lv1show");
                assert!(!state.show_file_dirty);
                assert!(state.show_file_last_saved_at.is_some());
                assert_eq!(state.selected_scene_id, Some("1::Verse".to_string()));
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(
            events.try_recv().is_err(),
            "load should publish one full show state event"
        );
    }

    #[tokio::test]
    async fn load_show_file_marks_dirty_when_invalid_entries_are_pruned() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Intro");
        let file = show_file_with_scenes(&["Intro", "Missing"], Some("99::Missing"));

        load_show_file_from_dto(
            &show,
            std::path::PathBuf::from("/tmp/pruned.lv1show"),
            file,
            Some(lv1),
        )
        .await
        .unwrap();

        let event = events.recv().await.unwrap();
        match event {
            AppEvent::Show(ShowEvent::StateChanged { reason, state }) => {
                assert_eq!(reason, ShowProjectionReason::FileMetadata);
                assert!(state.show_file_dirty);
                assert_eq!(state.scene_configs.len(), 1);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn load_show_file_falls_back_when_saved_selection_is_absent() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Intro");
        let file = show_file_with_scenes(&["Intro", "Missing"], Some("99::Missing"));

        let result = load_show_file_from_dto(
            &show,
            std::path::PathBuf::from("/tmp/fallback.lv1show"),
            file,
            Some(lv1),
        )
        .await
        .unwrap();

        assert_eq!(result.selected_scene_id, Some("1::Intro".to_string()));
        let event = events.recv().await.unwrap();
        match event {
            AppEvent::Show(ShowEvent::StateChanged { state, .. }) => {
                assert_eq!(state.selected_scene_id, Some("1::Intro".to_string()));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}

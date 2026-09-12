use crate::projector::AppViewState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    Scenes,
    CueLists,
    Events,
    Logs,
    Settings,
}

#[derive(Default)]
pub struct PresentationState {
    snapshot: AppViewState,
    has_snapshot: bool,
    latest_command_id: u64,
    command_error: Option<String>,
}

impl PresentationState {
    pub fn snapshot(&self) -> &AppViewState {
        &self.snapshot
    }

    pub fn command_error(&self) -> Option<&str> {
        self.command_error.as_deref()
    }

    /// @cc [owner:mixxorz,label:architecture] snapshot-version-ordering
    /// The first projector snapshot MAY have any version. Afterward, only a snapshot whose
    /// `state_version` is strictly newer than the accepted version may replace presentation state.
    pub fn accept_snapshot(&mut self, snapshot: AppViewState) -> bool {
        let accepted = !self.has_snapshot || snapshot.state_version > self.snapshot.state_version;
        self.has_snapshot = true;
        if accepted {
            self.snapshot = snapshot;
        }
        accepted
    }

    pub fn begin_command(&mut self) -> u64 {
        let command_id = self.latest_command_id.wrapping_add(1);
        self.command_started(command_id);
        command_id
    }

    pub fn command_started(&mut self, command_id: u64) {
        if command_id > self.latest_command_id {
            self.latest_command_id = command_id;
            self.command_error = None;
        }
    }

    /// @cc [owner:mixxorz,label:product] latest-user-command-error
    /// Among overlapping user commands, only the latest-started request may clear or set the
    /// visible command error; an older late result MUST NOT overwrite a newer outcome.
    pub fn complete_command(&mut self, command_id: u64, result: Result<(), String>) -> bool {
        if command_id != self.latest_command_id {
            return false;
        }
        self.command_error = result.err();
        true
    }

    pub fn window_title(&self) -> String {
        format_session_window_title(&self.snapshot.show_file_name, self.snapshot.show_file_dirty)
    }

    pub fn cued_scene_is_valid(&self) -> bool {
        let Some(active_list_id) = self.snapshot.active_cue_list_id.as_deref() else {
            return false;
        };
        let Some(cued_entry_id) = self.snapshot.cued_cue_entry_id.as_deref() else {
            return false;
        };
        let Some(entry) = self
            .snapshot
            .cue_lists
            .iter()
            .find(|list| list.id.to_string() == active_list_id)
            .and_then(|list| {
                list.entries
                    .iter()
                    .find(|entry| entry.id.to_string() == cued_entry_id)
            })
        else {
            return false;
        };
        self.snapshot
            .scene_configs
            .iter()
            .any(|scene| scene.internal_scene_id == entry.scene_internal_id)
    }
}

/// @cc [owner:mixxorz,label:product;formatting] session-window-title-state
/// The title MUST use the projected show-file name with only its final extension removed, and MUST
/// append ` *` if and only if the projected session is dirty; it MUST NOT infer state from a path
/// or local save operation.
pub fn format_session_window_title(show_file_name: &str, dirty: bool) -> String {
    let session_name = show_file_name
        .rsplit_once('.')
        .map_or(show_file_name, |(stem, _)| stem);
    format!(
        "Advanced Show Control - {session_name}{}",
        if dirty { " *" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cue_lists::{CueEntry, CueList};
    use crate::scenes::{SceneConfig, SceneScopeToggles};
    use uuid::Uuid;

    fn snapshot(version: u64, name: &str) -> AppViewState {
        AppViewState {
            state_version: version,
            show_file_name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn first_snapshot_accepts_any_version_then_requires_strictly_newer_versions() {
        let mut state = PresentationState::default();

        assert!(state.accept_snapshot(snapshot(0, "First")));
        assert!(!state.accept_snapshot(snapshot(0, "Equal")));
        assert!(!state.accept_snapshot(snapshot(0, "Older")));
        assert!(state.accept_snapshot(snapshot(1, "Newer")));
        assert_eq!(state.snapshot().show_file_name, "Newer");
    }

    #[test]
    fn older_command_result_cannot_replace_latest_command_error() {
        let mut state = PresentationState::default();
        let first = state.begin_command();
        let second = state.begin_command();

        assert!(state.complete_command(second, Err("new failure".to_string())));
        assert!(!state.complete_command(first, Ok(())));

        assert_eq!(state.command_error(), Some("new failure"));
    }

    #[test]
    fn title_marks_dirty_session() {
        assert_eq!(
            format_session_window_title("Show.ascs", true),
            "Advanced Show Control - Show *"
        );
        assert_eq!(
            format_session_window_title("Show.ascs", false),
            "Advanced Show Control - Show"
        );
    }

    #[test]
    fn cued_scene_is_valid_only_when_active_entry_resolves_to_scene_config() {
        let list_id = Uuid::new_v4();
        let entry_id = Uuid::new_v4();
        let scene_id = Uuid::new_v4();
        let mut state = PresentationState::default();
        let mut view = snapshot(1, "Show");
        view.active_cue_list_id = Some(list_id.to_string());
        view.cued_cue_entry_id = Some(entry_id.to_string());
        view.cue_lists = vec![CueList {
            id: list_id,
            name: "Main".to_string(),
            entries: vec![CueEntry {
                id: entry_id,
                scene_internal_id: scene_id,
            }],
        }];
        assert!(state.accept_snapshot(view.clone()));
        assert!(!state.cued_scene_is_valid());

        view.state_version = 2;
        view.scene_configs.push(SceneConfig {
            internal_scene_id: scene_id,
            scene_index: Some(1),
            scene_name: "Intro".to_string(),
            duration_ms: 1_000,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        });
        assert!(state.accept_snapshot(view));
        assert!(state.cued_scene_is_valid());
    }
}

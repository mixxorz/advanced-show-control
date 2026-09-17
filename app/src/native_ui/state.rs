use std::collections::{HashSet, VecDeque};

use uuid::Uuid;

use crate::projector::AppViewState;

pub(super) const GO_SUBMISSION_CAPACITY: usize = 8;

/// @cc [owner:mixxorz,label:safety;product] go-outstanding-command-capacity
/// Pointer and keyboard GO submission MUST share one eight-command capacity guard. Only a matching
/// command completion may release its slot; unrelated or duplicate completions MUST NOT change the
/// outstanding count. A submission MUST appear pending immediately. Only a newer matching cue
/// projection may advance its presentation, and only its command result may release capacity; the
/// two facts MAY arrive in either order and MUST be reconciled without double-advancing.
#[derive(Default)]
pub(super) struct GoSubmissionGuard {
    command_ids: HashSet<u64>,
    presentation_queue: VecDeque<GoPresentationEntry>,
}

struct GoPresentationEntry {
    command_id: u64,
    started_state_version: u64,
    started_session_revision: u64,
    intended_entry_id: Option<Uuid>,
    command_succeeded: bool,
    snapshot_acknowledged: bool,
}

impl GoSubmissionGuard {
    pub fn can_submit(&self) -> bool {
        self.command_ids.len() < GO_SUBMISSION_CAPACITY
    }

    pub fn presentation_pending_count(&self) -> usize {
        self.presentation_queue
            .iter()
            .filter(|entry| !entry.snapshot_acknowledged)
            .count()
    }

    pub fn start(
        &mut self,
        command_id: u64,
        state_version: u64,
        session_revision: u64,
        intended_entry_id: Uuid,
    ) {
        debug_assert!(self.can_submit());
        let inserted = self.command_ids.insert(command_id);
        debug_assert!(inserted);
        self.presentation_queue.push_back(GoPresentationEntry {
            command_id,
            started_state_version: state_version,
            started_session_revision: session_revision,
            intended_entry_id: Some(intended_entry_id),
            command_succeeded: false,
            snapshot_acknowledged: false,
        });
    }

    pub fn finish(
        &mut self,
        command_id: u64,
        recalled_entry_id: Option<Uuid>,
        snapshot: &AppViewState,
    ) -> bool {
        if !self.command_ids.remove(&command_id) {
            return false;
        }
        let Some(index) = self
            .presentation_queue
            .iter()
            .position(|entry| entry.command_id == command_id)
        else {
            return true;
        };
        if let Some(recalled_entry_id) = recalled_entry_id {
            if self.presentation_queue[index].intended_entry_id != Some(recalled_entry_id) {
                self.rebase_from(index, recalled_entry_id, snapshot);
            }
            self.presentation_queue[index].command_succeeded = true;
        } else {
            let failed_entry_id = self.presentation_queue[index].intended_entry_id;
            self.presentation_queue.remove(index);
            if let Some(failed_entry_id) = failed_entry_id {
                self.rebase_from(index, failed_entry_id, snapshot);
            }
        }
        self.observe_snapshot(snapshot);
        self.prune_acknowledged_successes();
        true
    }

    pub fn observe_snapshot(&mut self, snapshot: &AppViewState) -> bool {
        if self
            .presentation_queue
            .iter()
            .any(|entry| entry.started_session_revision != snapshot.session_revision)
        {
            return self.invalidate();
        }
        let Some(current_entry_id) = snapshot
            .current_cue_entry_id
            .as_deref()
            .and_then(|id| Uuid::parse_str(id).ok())
        else {
            return false;
        };
        let projected_next_entry_id = snapshot
            .cued_cue_entry_id
            .as_deref()
            .and_then(|id| Uuid::parse_str(id).ok());
        if projected_next_entry_id == Some(current_entry_id) {
            return false;
        }
        let Some(index) = self.presentation_queue.iter().rposition(|entry| {
            entry.intended_entry_id == Some(current_entry_id)
                && snapshot.state_version > entry.started_state_version
        }) else {
            return false;
        };
        let mut changed = false;
        for entry in self.presentation_queue.iter_mut().take(index + 1) {
            changed |= !entry.snapshot_acknowledged;
            entry.snapshot_acknowledged = true;
        }
        self.prune_acknowledged_successes();
        changed
    }

    pub fn invalidate(&mut self) -> bool {
        let changed = !self.command_ids.is_empty() || !self.presentation_queue.is_empty();
        self.command_ids.clear();
        self.presentation_queue.clear();
        changed
    }

    fn rebase_from(&mut self, index: usize, first_entry_id: Uuid, snapshot: &AppViewState) {
        let Some(active_list_id) = snapshot.active_cue_list_id.as_deref() else {
            return;
        };
        let Some(entries) = snapshot
            .cue_lists
            .iter()
            .find(|list| list.id.to_string() == active_list_id)
            .map(|list| list.entries.as_slice())
        else {
            return;
        };
        let Some(first_index) = entries.iter().position(|entry| entry.id == first_entry_id) else {
            return;
        };
        let mut ids = entries.iter().skip(first_index).map(|entry| entry.id);
        for entry in self.presentation_queue.iter_mut().skip(index) {
            entry.intended_entry_id = ids.next();
            entry.snapshot_acknowledged = false;
        }
    }

    fn prune_acknowledged_successes(&mut self) {
        while self
            .presentation_queue
            .front()
            .is_some_and(|entry| entry.command_succeeded && entry.snapshot_acknowledged)
        {
            self.presentation_queue.pop_front();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    Scenes,
    CueLists,
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

    fn cue_snapshot(version: u64, current: Uuid, next: Option<Uuid>) -> AppViewState {
        let list_id = Uuid::from_u128(100);
        let mut entry_ids = vec![current];
        if let Some(next) = next
            && next != current
        {
            entry_ids.push(next);
        }
        AppViewState {
            state_version: version,
            current_cue_entry_id: Some(current.to_string()),
            cued_cue_entry_id: next.map(|id| id.to_string()),
            active_cue_list_id: Some(list_id.to_string()),
            cue_lists: vec![CueList {
                id: list_id,
                name: "Main".to_string(),
                entries: entry_ids
                    .into_iter()
                    .map(|id| CueEntry {
                        id,
                        scene_internal_id: Uuid::new_v4(),
                    })
                    .collect(),
            }],
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
    fn go_presentation_waits_for_both_success_and_its_projection_in_either_order() {
        let recalled_entry_id = Uuid::from_u128(1);
        let next_entry_id = Uuid::from_u128(2);
        let old_snapshot = AppViewState {
            state_version: 10,
            cued_cue_entry_id: Some(recalled_entry_id.to_string()),
            ..Default::default()
        };
        let advanced_snapshot = cue_snapshot(11, recalled_entry_id, Some(next_entry_id));

        let mut completion_first = GoSubmissionGuard::default();
        completion_first.start(1, old_snapshot.state_version, 0, recalled_entry_id);
        assert!(completion_first.finish(1, Some(recalled_entry_id), &old_snapshot));
        assert!(completion_first.can_submit());
        assert_eq!(completion_first.presentation_pending_count(), 1);
        assert!(completion_first.observe_snapshot(&advanced_snapshot));
        assert_eq!(completion_first.presentation_pending_count(), 0);

        let mut projection_first = GoSubmissionGuard::default();
        projection_first.start(1, old_snapshot.state_version, 0, recalled_entry_id);
        assert!(projection_first.observe_snapshot(&advanced_snapshot));
        assert_eq!(projection_first.presentation_pending_count(), 0);
        assert!(projection_first.finish(1, Some(recalled_entry_id), &advanced_snapshot));
        assert!(projection_first.can_submit());
        assert_eq!(projection_first.presentation_pending_count(), 0);
    }

    #[test]
    fn session_replacement_discards_success_waiting_for_its_projection() {
        let recalled_entry_id = Uuid::from_u128(1);
        let old_snapshot = AppViewState {
            state_version: 10,
            cued_cue_entry_id: Some(recalled_entry_id.to_string()),
            ..Default::default()
        };
        let mut guard = GoSubmissionGuard::default();
        guard.start(1, old_snapshot.state_version, 0, recalled_entry_id);
        assert!(guard.finish(1, Some(recalled_entry_id), &old_snapshot));
        assert_eq!(guard.presentation_pending_count(), 1);

        let replacement = AppViewState {
            state_version: 11,
            session_revision: 1,
            ..Default::default()
        };
        assert!(guard.observe_snapshot(&replacement));
        assert_eq!(guard.presentation_pending_count(), 0);
        assert!(guard.can_submit());

        let mut completion_late = GoSubmissionGuard::default();
        completion_late.start(2, old_snapshot.state_version, 0, recalled_entry_id);
        assert!(completion_late.observe_snapshot(&replacement));
        assert!(!completion_late.finish(2, None, &replacement));
    }

    #[test]
    fn late_failure_reconciles_a_later_success_against_the_latest_projection() {
        let recalled_entry_id = Uuid::from_u128(1);
        let next_entry_id = Uuid::from_u128(2);
        let advanced_snapshot = cue_snapshot(11, recalled_entry_id, Some(next_entry_id));
        let mut guard = GoSubmissionGuard::default();
        guard.start(1, 10, 0, recalled_entry_id);
        guard.start(2, 10, 0, next_entry_id);

        assert!(guard.finish(2, Some(recalled_entry_id), &advanced_snapshot));
        assert_eq!(guard.presentation_pending_count(), 0);
        assert!(guard.finish(1, None, &advanced_snapshot));
        assert_eq!(guard.presentation_pending_count(), 0);
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

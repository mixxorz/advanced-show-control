use std::collections::VecDeque;

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
    presentation_queue: VecDeque<GoPresentationEntry>,
}

struct GoPresentationEntry {
    command_id: u64,
    started_state_version: u64,
    intended_entry_id: Option<Uuid>,
    phase: GoAcknowledgments,
}

/// Command admission and projected presentation are independent acknowledgments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GoAcknowledgments {
    AwaitingBoth,
    AwaitingProjection,
    AwaitingCommand,
    Complete,
}

impl GoAcknowledgments {
    fn command_pending(self) -> bool {
        matches!(self, Self::AwaitingBoth | Self::AwaitingCommand)
    }

    fn projection_pending(self) -> bool {
        matches!(self, Self::AwaitingBoth | Self::AwaitingProjection)
    }

    fn command_succeeded(&mut self) {
        *self = match self {
            Self::AwaitingBoth | Self::AwaitingProjection => Self::AwaitingProjection,
            Self::AwaitingCommand | Self::Complete => Self::Complete,
        };
    }

    fn projection_received(&mut self) {
        *self = match self {
            Self::AwaitingBoth | Self::AwaitingCommand => Self::AwaitingCommand,
            Self::AwaitingProjection | Self::Complete => Self::Complete,
        };
    }

    fn rebase_projection(&mut self) {
        *self = if self.command_pending() {
            Self::AwaitingBoth
        } else {
            Self::AwaitingProjection
        };
    }
}

impl GoSubmissionGuard {
    pub fn can_submit(&self) -> bool {
        self.presentation_queue
            .iter()
            .filter(|entry| entry.phase.command_pending())
            .count()
            < GO_SUBMISSION_CAPACITY
    }

    pub fn presentation_pending_count(&self) -> usize {
        self.presentation_queue
            .iter()
            .filter(|entry| entry.phase.projection_pending())
            .count()
    }

    pub fn start(&mut self, command_id: u64, state_version: u64, intended_entry_id: Uuid) {
        debug_assert!(self.can_submit());
        debug_assert!(
            !self
                .presentation_queue
                .iter()
                .any(|entry| { entry.command_id == command_id && entry.phase.command_pending() })
        );
        self.presentation_queue.push_back(GoPresentationEntry {
            command_id,
            started_state_version: state_version,
            intended_entry_id: Some(intended_entry_id),
            phase: GoAcknowledgments::AwaitingBoth,
        });
    }

    pub fn finish(
        &mut self,
        command_id: u64,
        recalled_entry_id: Option<Uuid>,
        snapshot: &AppViewState,
    ) -> bool {
        let Some(index) = self
            .presentation_queue
            .iter()
            .position(|entry| entry.command_id == command_id && entry.phase.command_pending())
        else {
            return false;
        };
        if let Some(recalled_entry_id) = recalled_entry_id {
            if self.presentation_queue[index].intended_entry_id != Some(recalled_entry_id) {
                self.rebase_from(index, recalled_entry_id, snapshot);
            }
            self.presentation_queue[index].phase.command_succeeded();
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
        let Some(current_entry_id) = snapshot.current_cue_entry_id else {
            return false;
        };
        let projected_next_entry_id = snapshot.cued_cue_entry_id;
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
            changed |= entry.phase.projection_pending();
            entry.phase.projection_received();
        }
        self.prune_acknowledged_successes();
        changed
    }

    fn rebase_from(&mut self, index: usize, first_entry_id: Uuid, snapshot: &AppViewState) {
        let Some(active_list_id) = snapshot.active_cue_list_id else {
            return;
        };
        let Some(entries) = snapshot
            .cue_lists
            .iter()
            .find(|list| list.id == active_list_id)
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
            entry.phase.rebase_projection();
        }
    }

    fn prune_acknowledged_successes(&mut self) {
        while self
            .presentation_queue
            .front()
            .is_some_and(|entry| entry.phase == GoAcknowledgments::Complete)
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
enum SnapshotPhase {
    #[default]
    Uninitialized,
    Initialized,
}

#[derive(Default)]
enum CommandOutcome {
    Pending,
    #[default]
    Succeeded,
    Failed(String),
}

#[derive(Default)]
pub struct PresentationState {
    snapshot: AppViewState,
    snapshot_phase: SnapshotPhase,
    latest_command_id: u64,
    command_outcome: CommandOutcome,
}

impl PresentationState {
    pub fn snapshot(&self) -> &AppViewState {
        &self.snapshot
    }

    pub fn command_error(&self) -> Option<&str> {
        match &self.command_outcome {
            CommandOutcome::Failed(error) => Some(error.as_str()),
            CommandOutcome::Pending | CommandOutcome::Succeeded => None,
        }
    }

    /// @cc [owner:mixxorz,label:architecture] snapshot-version-ordering
    /// The first projector snapshot MAY have any version. Afterward, only a snapshot whose
    /// `state_version` is strictly newer than the accepted version may replace presentation state.
    pub fn accept_snapshot(&mut self, snapshot: AppViewState) -> bool {
        let accepted = matches!(self.snapshot_phase, SnapshotPhase::Uninitialized)
            || snapshot.state_version > self.snapshot.state_version;
        self.snapshot_phase = SnapshotPhase::Initialized;
        if accepted {
            self.snapshot = snapshot;
        }
        accepted
    }

    pub fn command_started(&mut self, command_id: u64) {
        if command_id > self.latest_command_id {
            self.latest_command_id = command_id;
            self.command_outcome = CommandOutcome::Pending;
        }
    }

    /// @cc [owner:mixxorz,label:product] latest-user-command-error
    /// Among overlapping user commands, only the latest-started request may clear or set the
    /// visible command error; an older late result MUST NOT overwrite a newer outcome.
    pub fn complete_command(&mut self, command_id: u64, result: Result<(), String>) -> bool {
        if command_id != self.latest_command_id {
            return false;
        }
        self.command_outcome = match result {
            Ok(()) => CommandOutcome::Succeeded,
            Err(error) => CommandOutcome::Failed(error),
        };
        true
    }

    pub fn window_title(&self) -> String {
        format_session_window_title(&self.snapshot.show_file_name, self.snapshot.show_file_dirty)
    }
}

/// @cc [owner:mixxorz,label:product;formatting] session-window-title-state
/// The title MUST use the projected show-file name with only its final extension removed, and MUST
/// append ` *` if and only if the projected session is dirty; it MUST NOT infer state from a path
/// or local save operation.
pub fn format_session_window_title(show_file_name: &str, dirty: bool) -> String {
    let session_name = session_display_name(show_file_name);
    format!(
        "Advanced Show Control - {session_name}{}",
        if dirty { " *" } else { "" }
    )
}

pub(super) fn session_display_name(show_file_name: &str) -> &str {
    show_file_name
        .rsplit_once('.')
        .map_or(show_file_name, |(stem, _)| stem)
}

/// An optimistic value stays local until its authoritative projection arrives.
/// Successful command completion does not itself acknowledge the displayed value.
#[derive(Clone, Debug, Default)]
pub(super) enum ProjectedEdit<T> {
    #[default]
    Projected,
    Submitting { command_id: u64, value: T },
    AwaitingProjection { command_id: u64, value: T },
}

impl<T> ProjectedEdit<T> {
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Projected => None,
            Self::Submitting { value, .. } | Self::AwaitingProjection { value, .. } => Some(value),
        }
    }

    pub fn submit(&mut self, command_id: u64, value: T) {
        *self = Self::Submitting { command_id, value };
    }

    pub fn clear(&mut self) {
        *self = Self::Projected;
    }

    /// Returns true only when the current edit failed and was discarded.
    pub fn complete(&mut self, command_id: u64, failed: bool) -> bool {
        let current = match self {
            Self::Projected => return false,
            Self::Submitting { command_id, .. } | Self::AwaitingProjection { command_id, .. } => {
                *command_id
            }
        };
        if current != command_id {
            return false;
        }
        if failed {
            self.clear();
            return true;
        }
        *self = match std::mem::take(self) {
            Self::Submitting { command_id, value } => {
                Self::AwaitingProjection { command_id, value }
            }
            state => state,
        };
        false
    }
}

/// Blur is deferred one frame; explicit edits invalidate that scheduled transition.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct DeferredCommit {
    revision: u64,
    phase: DeferredCommitPhase,
}

#[derive(Clone, Copy, Debug, Default)]
enum DeferredCommitPhase {
    #[default]
    Editing,
    Scheduled(u64),
}

impl DeferredCommit {
    pub fn schedule(&mut self) -> u64 {
        self.phase = DeferredCommitPhase::Scheduled(self.revision);
        self.revision
    }

    pub fn take_scheduled(&mut self, revision: u64) -> bool {
        if matches!(self.phase, DeferredCommitPhase::Scheduled(current) if current == revision)
            && self.revision == revision
        {
            self.phase = DeferredCommitPhase::Editing;
            true
        } else {
            false
        }
    }

    pub fn cancel(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.phase = DeferredCommitPhase::Editing;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cue_lists::{CueEntry, CueList};
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
            current_cue_entry_id: Some(current),
            cued_cue_entry_id: next,
            active_cue_list_id: Some(list_id),
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
        assert!(state.accept_snapshot(snapshot(1, "Newer")));
        assert!(!state.accept_snapshot(snapshot(0, "Older")));
        assert_eq!(state.snapshot().show_file_name, "Newer");
    }

    #[test]
    fn go_presentation_waits_for_both_success_and_its_projection_in_either_order() {
        let recalled_entry_id = Uuid::from_u128(1);
        let next_entry_id = Uuid::from_u128(2);
        let old_snapshot = AppViewState {
            state_version: 10,
            cued_cue_entry_id: Some(recalled_entry_id),
            ..Default::default()
        };
        let advanced_snapshot = cue_snapshot(11, recalled_entry_id, Some(next_entry_id));

        let mut completion_first = GoSubmissionGuard::default();
        completion_first.start(1, old_snapshot.state_version, recalled_entry_id);
        assert!(completion_first.finish(1, Some(recalled_entry_id), &old_snapshot));
        assert!(completion_first.can_submit());
        assert_eq!(completion_first.presentation_pending_count(), 1);
        assert!(completion_first.observe_snapshot(&advanced_snapshot));
        assert_eq!(completion_first.presentation_pending_count(), 0);

        let mut projection_first = GoSubmissionGuard::default();
        projection_first.start(1, old_snapshot.state_version, recalled_entry_id);
        assert!(projection_first.observe_snapshot(&advanced_snapshot));
        assert_eq!(projection_first.presentation_pending_count(), 0);
        assert!(projection_first.finish(1, Some(recalled_entry_id), &advanced_snapshot));
        assert!(projection_first.can_submit());
        assert_eq!(projection_first.presentation_pending_count(), 0);
    }

    #[test]
    fn late_failure_reconciles_a_later_success_against_the_latest_projection() {
        let recalled_entry_id = Uuid::from_u128(1);
        let next_entry_id = Uuid::from_u128(2);
        let advanced_snapshot = cue_snapshot(11, recalled_entry_id, Some(next_entry_id));
        let mut guard = GoSubmissionGuard::default();
        guard.start(1, 10, recalled_entry_id);
        guard.start(2, 10, next_entry_id);

        assert!(guard.finish(2, Some(recalled_entry_id), &advanced_snapshot));
        assert_eq!(guard.presentation_pending_count(), 0);
        assert!(guard.finish(1, None, &advanced_snapshot));
        assert_eq!(guard.presentation_pending_count(), 0);
    }

    #[test]
    fn older_command_result_cannot_replace_latest_command_error() {
        let mut state = PresentationState::default();
        let first = 1;
        let second = 2;
        state.command_started(first);
        state.command_started(second);

        assert!(state.complete_command(second, Err("new failure".to_string())));
        assert!(!state.complete_command(first, Ok(())));

        assert_eq!(state.command_error(), Some("new failure"));
    }

    #[test]
    fn session_display_name_omits_the_file_extension() {
        assert_eq!(session_display_name("Tour.Show.ascs"), "Tour.Show");
        assert_eq!(session_display_name("Untitled Session"), "Untitled Session");
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
}

#[cfg(test)]
mod edit_state_tests {
    use super::{DeferredCommit, ProjectedEdit};

    #[test]
    fn optimistic_edit_waits_for_projection_and_ignores_old_failures() {
        let mut edit = ProjectedEdit::default();
        edit.submit(1, "first");
        edit.submit(2, "second");
        assert!(!edit.complete(1, true));
        assert_eq!(edit.value(), Some(&"second"));
        assert!(!edit.complete(2, false));
        assert!(!edit.complete(2, false));
        assert_eq!(edit.value(), Some(&"second"));
        edit.clear();
        assert!(!edit.complete(2, true));
        assert!(edit.value().is_none());
        edit.submit(3, "third");
        assert!(edit.complete(3, true));
        assert!(edit.value().is_none());
    }

    #[test]
    fn explicit_edit_invalidates_blur_without_consuming_a_newer_blur() {
        let mut commit = DeferredCommit::default();
        let old = commit.schedule();
        commit.cancel();
        let new = commit.schedule();
        assert!(!commit.take_scheduled(old));
        assert!(commit.take_scheduled(new));
        assert!(!commit.take_scheduled(new));
    }
}

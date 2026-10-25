use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SessionAction {
    New,
    NewFromTemplate,
    Open,
    Quit,
    UpdateAndRestart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GuardChoice {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct SessionStatus {
    pub path: Option<PathBuf>,
    pub name: String,
    pub dirty: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum GuardEffect {
    None,
    QueryState,
    Prompt(String),
    ChooseSaveDestination,
    SaveCurrent,
    SaveTo(PathBuf),
    Continue(SessionAction),
    Error(String),
}

#[derive(Clone, Copy)]
enum QueryKind {
    Preflight,
    PostSave,
}

enum GuardPhase {
    Querying {
        kind: QueryKind,
        query_id: Option<u64>,
    },
    AwaitingChoice {
        titled: bool,
    },
    AwaitingSaveDestination,
    Saving {
        command_id: Option<u64>,
    },
    Continuing {
        command_id: Option<u64>,
    },
}

struct PendingAction {
    action: SessionAction,
    phase: GuardPhase,
}

#[derive(Default)]
pub(super) struct SessionGuard {
    pending: Option<PendingAction>,
    busy: Arc<AtomicBool>,
}

impl SessionGuard {
    pub(super) fn new(busy: Arc<AtomicBool>) -> Self {
        Self {
            pending: None,
            busy,
        }
    }

    /// @cc [owner:mixxorz,label:product;persistence] dirty-session-destructive-action-gate
    /// Every destructive session action MUST first obtain an authoritative Show session-state
    /// result. It may continue only when that preflight is clean, the user explicitly chose
    /// Discard, or a successful save is followed by an authoritative clean recheck. The existing
    /// pending state MUST block new UI document edits and GO until the picker and continued file
    /// operation finish, or the action is cancelled or fails. Uncorrelated results MUST NOT advance
    /// or clear the pending action; query/save failure and a dirty post-save recheck MUST NOT continue.
    pub(super) fn request(&mut self, action: SessionAction) -> GuardEffect {
        if self.pending.is_some() {
            return GuardEffect::None;
        }
        self.busy.store(true, Ordering::Relaxed);
        self.pending = Some(PendingAction {
            action,
            phase: GuardPhase::Querying {
                kind: QueryKind::Preflight,
                query_id: None,
            },
        });
        GuardEffect::QueryState
    }

    /// @cc [owner:mixxorz,label:product;persistence] dirty-window-close-veto
    /// A platform close request MUST always be vetoed. The first request enters the same
    /// authoritative preflight as menu and keyboard actions; repeated requests remain vetoed while
    /// preflight, prompting, saving, post-save verification, or continuation is pending. Closing
    /// occurs only by the guarded Quit continuation.
    pub(super) fn request_close(&mut self) -> (bool, GuardEffect) {
        (false, self.request(SessionAction::Quit))
    }

    pub(super) fn query_started(&mut self, query_id: u64) {
        if let Some(PendingAction {
            phase: GuardPhase::Querying { query_id: slot, .. },
            ..
        }) = self.pending.as_mut()
            && slot.is_none()
        {
            *slot = Some(query_id);
        }
    }

    pub(super) fn cancel_pending(&mut self) -> bool {
        self.busy.store(false, Ordering::Relaxed);
        self.pending.take().is_some()
    }

    pub(super) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn state_query_finished(
        &mut self,
        query_id: u64,
        result: Result<SessionStatus, String>,
    ) -> GuardEffect {
        let Some(PendingAction {
            action,
            phase:
                GuardPhase::Querying {
                    kind,
                    query_id: Some(expected_id),
                },
        }) = self.pending.as_ref()
        else {
            return GuardEffect::None;
        };
        if *expected_id != query_id {
            return GuardEffect::None;
        }
        let action = *action;
        let kind = *kind;
        let status = match result {
            Ok(status) => status,
            Err(error) => {
                self.cancel_pending();
                return GuardEffect::Error(format!(
                    "Could not check the current session before continuing: {error}"
                ));
            }
        };
        if !status.dirty {
            self.pending.as_mut().expect("pending action exists").phase =
                GuardPhase::Continuing { command_id: None };
            return GuardEffect::Continue(action);
        }
        match kind {
            QueryKind::Preflight => {
                self.pending.as_mut().expect("pending action exists").phase =
                    GuardPhase::AwaitingChoice {
                        titled: status.path.is_some(),
                    };
                GuardEffect::Prompt(status.name)
            }
            QueryKind::PostSave => {
                self.cancel_pending();
                GuardEffect::Error(
                    "The session changed while it was being saved. The requested action was cancelled."
                        .to_string(),
                )
            }
        }
    }

    pub(super) fn choose(&mut self, choice: GuardChoice) -> GuardEffect {
        let Some(PendingAction {
            action,
            phase: GuardPhase::AwaitingChoice { titled },
        }) = self.pending.as_ref()
        else {
            return GuardEffect::None;
        };
        let action = *action;
        let titled = *titled;
        match choice {
            GuardChoice::Save if titled => {
                self.pending.as_mut().expect("pending action exists").phase =
                    GuardPhase::Saving { command_id: None };
                GuardEffect::SaveCurrent
            }
            GuardChoice::Save => {
                self.pending.as_mut().expect("pending action exists").phase =
                    GuardPhase::AwaitingSaveDestination;
                GuardEffect::ChooseSaveDestination
            }
            GuardChoice::Discard => {
                self.pending.as_mut().expect("pending action exists").phase =
                    GuardPhase::Continuing { command_id: None };
                GuardEffect::Continue(action)
            }
            GuardChoice::Cancel => {
                self.cancel_pending();
                GuardEffect::None
            }
        }
    }

    pub(super) fn save_destination(&mut self, path: Option<PathBuf>) -> GuardEffect {
        if !matches!(
            self.pending.as_ref().map(|pending| &pending.phase),
            Some(GuardPhase::AwaitingSaveDestination)
        ) {
            return GuardEffect::None;
        }
        match path {
            Some(path) => {
                self.pending.as_mut().expect("pending action exists").phase =
                    GuardPhase::Saving { command_id: None };
                GuardEffect::SaveTo(path)
            }
            None => {
                self.cancel_pending();
                GuardEffect::None
            }
        }
    }

    pub(super) fn save_started(&mut self, command_id: u64) {
        if let Some(PendingAction {
            phase: GuardPhase::Saving { command_id: slot },
            ..
        }) = self.pending.as_mut()
            && slot.is_none()
        {
            *slot = Some(command_id);
        }
    }

    pub(super) fn continuation_started(&mut self, command_id: u64) {
        if let Some(PendingAction {
            phase: GuardPhase::Continuing { command_id: slot },
            ..
        }) = self.pending.as_mut()
            && slot.is_none()
        {
            *slot = Some(command_id);
        }
    }

    pub(super) fn is_continuing(&self) -> bool {
        matches!(
            self.pending.as_ref().map(|pending| &pending.phase),
            Some(GuardPhase::Continuing { .. })
        )
    }

    pub(super) fn command_finished(&mut self, command_id: u64, succeeded: bool) -> GuardEffect {
        let continuation_matches = matches!(
            self.pending.as_ref().map(|pending| &pending.phase),
            Some(GuardPhase::Continuing { command_id: Some(expected_id) })
                if *expected_id == command_id
        );
        if continuation_matches {
            self.cancel_pending();
            return GuardEffect::None;
        }
        let failed_query_matches = matches!(
            self.pending.as_ref().map(|pending| &pending.phase),
            Some(GuardPhase::Querying {
                query_id: Some(expected_id),
                ..
            }) if *expected_id == command_id && !succeeded
        );
        if failed_query_matches {
            self.cancel_pending();
            return GuardEffect::None;
        }
        let matches = matches!(
            self.pending.as_ref().map(|pending| &pending.phase),
            Some(GuardPhase::Saving {
                command_id: Some(expected_id)
            }) if *expected_id == command_id
        );
        if !matches {
            return GuardEffect::None;
        }
        if !succeeded {
            self.cancel_pending();
            return GuardEffect::None;
        }
        self.pending.as_mut().expect("pending action exists").phase = GuardPhase::Querying {
            kind: QueryKind::PostSave,
            query_id: None,
        };
        GuardEffect::QueryState
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{GuardChoice, GuardEffect, SessionAction, SessionGuard, SessionStatus};

    fn status(dirty: bool, titled: bool) -> SessionStatus {
        SessionStatus {
            dirty,
            path: titled.then(|| PathBuf::from("show.ascs")),
            name: if titled {
                "Show.ascs"
            } else {
                "Untitled Session"
            }
            .to_string(),
        }
    }

    #[test]
    fn update_and_restart_requires_clean_preflight_and_cancels_like_quit() {
        let mut guard = SessionGuard::default();
        assert_eq!(
            guard.request(SessionAction::UpdateAndRestart),
            GuardEffect::QueryState
        );
        guard.query_started(1);
        assert_eq!(
            guard.state_query_finished(1, Ok(status(true, true))),
            GuardEffect::Prompt("Show.ascs".to_string())
        );
        assert_eq!(guard.choose(GuardChoice::Cancel), GuardEffect::None);
        assert!(!guard.is_pending());

        guard.request(SessionAction::UpdateAndRestart);
        guard.query_started(2);
        assert_eq!(
            guard.state_query_finished(2, Ok(status(false, true))),
            GuardEffect::Continue(SessionAction::UpdateAndRestart)
        );
    }

    #[test]
    fn every_request_enters_correlated_preflight() {
        let mut guard = SessionGuard::default();

        assert_eq!(guard.request(SessionAction::New), GuardEffect::QueryState);
        guard.query_started(10);
        assert_eq!(
            guard.state_query_finished(9, Ok(status(false, false))),
            GuardEffect::None
        );
        assert!(guard.is_pending());
        assert_eq!(
            guard.state_query_finished(10, Ok(status(false, false))),
            GuardEffect::Continue(SessionAction::New)
        );
        assert!(guard.is_pending());
        assert!(guard.busy.load(std::sync::atomic::Ordering::Relaxed));
        guard.continuation_started(11);
        assert_eq!(guard.request_close(), (false, GuardEffect::None));
        guard.command_finished(10, true);
        assert!(guard.is_pending());
        guard.command_finished(11, true);
        assert!(!guard.is_pending());
        assert!(!guard.busy.load(std::sync::atomic::Ordering::Relaxed));

        guard.request(SessionAction::Open);
        guard.query_started(12);
        guard.state_query_finished(12, Ok(status(false, true)));
        guard.continuation_started(13);
        guard.command_finished(13, false);
        assert!(!guard.is_pending());
        assert!(!guard.busy.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn close_is_vetoed_through_preflight_prompt_and_save() {
        let mut guard = SessionGuard::default();

        assert_eq!(guard.request_close(), (false, GuardEffect::QueryState));
        guard.query_started(1);
        assert_eq!(
            guard.state_query_finished(1, Ok(status(true, true))),
            GuardEffect::Prompt("Show.ascs".to_string())
        );
        assert_eq!(guard.request_close(), (false, GuardEffect::None));
        assert_eq!(guard.choose(GuardChoice::Save), GuardEffect::SaveCurrent);
        guard.save_started(2);
        assert_eq!(guard.request_close(), (false, GuardEffect::None));
        assert_eq!(guard.command_finished(2, true), GuardEffect::QueryState);
        guard.query_started(3);
        assert_eq!(guard.request_close(), (false, GuardEffect::None));
        assert_eq!(
            guard.state_query_finished(3, Ok(status(false, true))),
            GuardEffect::Continue(SessionAction::Quit)
        );
    }

    #[test]
    fn authoritative_preflight_path_selects_save_or_save_as() {
        let mut titled = SessionGuard::default();
        titled.request(SessionAction::Open);
        titled.query_started(1);
        assert_eq!(
            titled.state_query_finished(1, Ok(status(true, true))),
            GuardEffect::Prompt("Show.ascs".to_string())
        );
        assert_eq!(titled.choose(GuardChoice::Save), GuardEffect::SaveCurrent);

        let mut untitled = SessionGuard::default();
        untitled.request(SessionAction::Open);
        untitled.query_started(2);
        assert_eq!(
            untitled.state_query_finished(2, Ok(status(true, false))),
            GuardEffect::Prompt("Untitled Session".to_string())
        );
        assert_eq!(
            untitled.choose(GuardChoice::Save),
            GuardEffect::ChooseSaveDestination
        );
    }

    #[test]
    fn save_success_rechecks_and_aborts_if_session_remains_dirty() {
        let mut guard = SessionGuard::default();
        guard.request(SessionAction::NewFromTemplate);
        guard.query_started(1);
        guard.state_query_finished(1, Ok(status(true, false)));
        guard.choose(GuardChoice::Save);
        guard.save_destination(Some(PathBuf::from("saved.ascs")));
        guard.save_started(2);

        assert_eq!(guard.command_finished(2, true), GuardEffect::QueryState);
        guard.query_started(3);
        assert_eq!(
            guard.state_query_finished(3, Ok(status(true, true))),
            GuardEffect::Error(
                "The session changed while it was being saved. The requested action was cancelled."
                    .to_string()
            )
        );
        assert!(!guard.is_pending());
    }

    #[test]
    fn current_path_save_also_aborts_if_session_remains_dirty() {
        let mut guard = SessionGuard::default();
        guard.request(SessionAction::Open);
        guard.query_started(1);
        guard.state_query_finished(1, Ok(status(true, true)));
        assert_eq!(guard.choose(GuardChoice::Save), GuardEffect::SaveCurrent);
        guard.save_started(2);

        assert_eq!(guard.command_finished(2, true), GuardEffect::QueryState);
        guard.query_started(3);
        assert!(matches!(
            guard.state_query_finished(3, Ok(status(true, true))),
            GuardEffect::Error(_)
        ));
        assert!(!guard.is_pending());
    }

    #[test]
    fn stale_post_save_query_is_ignored() {
        let mut guard = SessionGuard::default();
        guard.request(SessionAction::Quit);
        guard.query_started(4);
        guard.state_query_finished(4, Ok(status(true, true)));
        guard.choose(GuardChoice::Save);
        guard.save_started(5);
        guard.command_finished(5, true);
        guard.query_started(6);

        assert_eq!(
            guard.state_query_finished(4, Ok(status(false, true))),
            GuardEffect::None
        );
        assert!(guard.is_pending());
    }

    #[test]
    fn modal_cancellation_clears_pending_action() {
        let mut guard = SessionGuard::default();
        guard.request(SessionAction::New);
        guard.query_started(5);

        assert!(guard.cancel_pending());
        assert!(!guard.is_pending());
        assert_eq!(
            guard.state_query_finished(5, Ok(status(false, false))),
            GuardEffect::None
        );
    }

    #[test]
    fn matching_generic_query_failure_clears_pending_intent() {
        let mut guard = SessionGuard::default();
        guard.request(SessionAction::New);
        guard.query_started(17);

        assert_eq!(guard.command_finished(17, false), GuardEffect::None);
        assert!(!guard.is_pending());
    }

    #[test]
    fn discard_continues_and_cancel_or_failures_clear_intent() {
        let mut guard = SessionGuard::default();
        guard.request(SessionAction::Open);
        guard.query_started(1);
        guard.state_query_finished(1, Ok(status(true, true)));
        assert_eq!(
            guard.choose(GuardChoice::Discard),
            GuardEffect::Continue(SessionAction::Open)
        );

        assert!(guard.is_continuing());
        assert!(guard.busy.load(std::sync::atomic::Ordering::Relaxed));
        guard.cancel_pending(); // The continued picker was cancelled.
        assert!(!guard.busy.load(std::sync::atomic::Ordering::Relaxed));
        guard.request(SessionAction::Quit);
        guard.query_started(2);
        guard.state_query_finished(2, Ok(status(true, true)));
        assert_eq!(guard.choose(GuardChoice::Cancel), GuardEffect::None);
        assert!(!guard.is_pending());

        guard.request(SessionAction::New);
        guard.query_started(3);
        assert!(matches!(
            guard.state_query_finished(3, Err("show unavailable".to_string())),
            GuardEffect::Error(_)
        ));
        assert!(!guard.is_pending());
    }
}

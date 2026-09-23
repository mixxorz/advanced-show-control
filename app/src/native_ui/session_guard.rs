use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SessionAction {
    New,
    NewFromTemplate,
    Open,
    Quit,
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
    pub persisted_session_revision: u64,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum GuardEffect {
    None,
    QueryState,
    Prompt(String),
    ChooseSaveDestination,
    SaveCurrent,
    SaveTo(PathBuf),
    Continue {
        action: SessionAction,
        expected_persisted_revision: Option<u64>,
    },
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
}

struct PendingAction {
    action: SessionAction,
    phase: GuardPhase,
}

#[derive(Default)]
pub(super) struct SessionGuard {
    pending: Option<PendingAction>,
}

impl SessionGuard {
    /// @cc [owner:mixxorz,label:product;persistence] dirty-session-destructive-action-gate
    /// Every destructive session action MUST first obtain an authoritative Show session-state
    /// result. It may continue only when that preflight is clean, the user explicitly chose
    /// Discard, or a successful save is followed by an authoritative clean recheck. A clean
    /// preflight or post-save continuation MUST carry the exact owner-side revision it validated;
    /// explicit Discard MUST carry no revision and remains unconditional. A matching query whose
    /// persisted-edit submission epoch or owner-side persisted revision is stale MUST retain its
    /// preflight or post-save phase and request another query. Uncorrelated results,
    /// cancellation, query/save failure, and
    /// a dirty post-save recheck MUST NOT continue the action.
    pub(super) fn request(&mut self, action: SessionAction) -> GuardEffect {
        if self.pending.is_some() {
            return GuardEffect::None;
        }
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
    /// preflight, prompting, saving, or post-save verification is pending. Closing occurs only by
    /// the guarded Quit continuation.
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

    pub(super) fn state_query_stale(&mut self, query_id: u64) -> GuardEffect {
        let Some(PendingAction {
            phase: GuardPhase::Querying { query_id: slot, .. },
            ..
        }) = self.pending.as_mut()
        else {
            return GuardEffect::None;
        };
        if *slot != Some(query_id) {
            return GuardEffect::None;
        }
        *slot = None;
        GuardEffect::QueryState
    }

    pub(super) fn cancel_pending(&mut self) -> bool {
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
                self.pending = None;
                return GuardEffect::Error(format!(
                    "Could not check the current session before continuing: {error}"
                ));
            }
        };
        match kind {
            QueryKind::Preflight if !status.dirty => {
                self.pending = None;
                GuardEffect::Continue {
                    action,
                    expected_persisted_revision: Some(status.persisted_session_revision),
                }
            }
            QueryKind::Preflight => {
                self.pending.as_mut().expect("pending action exists").phase =
                    GuardPhase::AwaitingChoice {
                        titled: status.path.is_some(),
                    };
                GuardEffect::Prompt(status.name)
            }
            QueryKind::PostSave if !status.dirty => {
                self.pending = None;
                GuardEffect::Continue {
                    action,
                    expected_persisted_revision: Some(status.persisted_session_revision),
                }
            }
            QueryKind::PostSave => {
                self.pending = None;
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
                self.pending = None;
                GuardEffect::Continue {
                    action,
                    expected_persisted_revision: None,
                }
            }
            GuardChoice::Cancel => {
                self.pending = None;
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
                self.pending = None;
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

    pub(super) fn command_finished(&mut self, command_id: u64, succeeded: bool) -> GuardEffect {
        let failed_query_matches = matches!(
            self.pending.as_ref().map(|pending| &pending.phase),
            Some(GuardPhase::Querying {
                query_id: Some(expected_id),
                ..
            }) if *expected_id == command_id && !succeeded
        );
        if failed_query_matches {
            self.pending = None;
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
            self.pending = None;
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
            persisted_session_revision: 9,
        }
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
            GuardEffect::Continue {
                action: SessionAction::New,
                expected_persisted_revision: Some(9),
            }
        );
        assert!(!guard.is_pending());
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
            GuardEffect::Continue {
                action: SessionAction::Quit,
                expected_persisted_revision: Some(9),
            }
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
    fn stale_preflight_query_retries_without_losing_the_action() {
        let mut guard = SessionGuard::default();
        guard.request(SessionAction::Open);
        guard.query_started(10);

        assert_eq!(guard.state_query_stale(10), GuardEffect::QueryState);
        assert!(guard.is_pending());
        guard.query_started(11);
        assert_eq!(
            guard.state_query_finished(11, Ok(status(false, true))),
            GuardEffect::Continue {
                action: SessionAction::Open,
                expected_persisted_revision: Some(9),
            }
        );
    }

    #[test]
    fn stale_post_save_query_retries_without_losing_the_phase() {
        let mut guard = SessionGuard::default();
        guard.request(SessionAction::Quit);
        guard.query_started(1);
        guard.state_query_finished(1, Ok(status(true, true)));
        guard.choose(GuardChoice::Save);
        guard.save_started(2);
        assert_eq!(guard.command_finished(2, true), GuardEffect::QueryState);
        guard.query_started(3);

        assert_eq!(guard.state_query_stale(3), GuardEffect::QueryState);
        guard.query_started(4);
        assert_eq!(
            guard.state_query_finished(4, Ok(status(false, true))),
            GuardEffect::Continue {
                action: SessionAction::Quit,
                expected_persisted_revision: Some(9),
            }
        );
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
            GuardEffect::Continue {
                action: SessionAction::Open,
                expected_persisted_revision: None,
            }
        );

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

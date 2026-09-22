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
pub(super) enum GuardEffect {
    None,
    Prompt,
    ChooseSaveDestination,
    SaveCurrent,
    SaveTo(PathBuf),
    Continue(SessionAction),
}

#[derive(Default)]
pub(super) struct SessionGuard {
    pending_action: Option<SessionAction>,
    save_command_id: Option<u64>,
}

impl SessionGuard {
    /// @cc [owner:mixxorz,label:product;persistence] dirty-session-destructive-action-gate
    /// A destructive session action MUST continue only when the projected session was clean, the
    /// user explicitly chose Discard, or the guard observed success for its own save command.
    /// Cancel, Save As cancellation, and save failure MUST clear pending intent without continuing.
    pub(super) fn request(&mut self, action: SessionAction, dirty: bool) -> GuardEffect {
        if self.pending_action.is_some() {
            return GuardEffect::None;
        }
        if !dirty {
            return GuardEffect::Continue(action);
        }
        self.pending_action = Some(action);
        GuardEffect::Prompt
    }

    /// @cc [owner:mixxorz,label:product;persistence] dirty-window-close-veto
    /// A platform close request MUST be accepted synchronously only for a clean session. A dirty
    /// close MUST be vetoed and enter the same guarded Quit flow as menu and keyboard actions.
    pub(super) fn request_close(&mut self, dirty: bool) -> (bool, GuardEffect) {
        if !dirty {
            return (true, GuardEffect::None);
        }
        (false, self.request(SessionAction::Quit, true))
    }

    pub(super) fn choose(&mut self, choice: GuardChoice, titled: bool) -> GuardEffect {
        let Some(action) = self.pending_action else {
            return GuardEffect::None;
        };
        match choice {
            GuardChoice::Save if titled => GuardEffect::SaveCurrent,
            GuardChoice::Save => GuardEffect::ChooseSaveDestination,
            GuardChoice::Discard => {
                self.pending_action = None;
                GuardEffect::Continue(action)
            }
            GuardChoice::Cancel => {
                self.pending_action = None;
                GuardEffect::None
            }
        }
    }

    pub(super) fn save_destination(&mut self, path: Option<PathBuf>) -> GuardEffect {
        if self.pending_action.is_none() {
            return GuardEffect::None;
        }
        match path {
            Some(path) => GuardEffect::SaveTo(path),
            None => {
                self.pending_action = None;
                GuardEffect::None
            }
        }
    }

    pub(super) fn save_started(&mut self, command_id: u64) {
        if self.pending_action.is_some() {
            self.save_command_id = Some(command_id);
        }
    }

    pub(super) fn command_finished(&mut self, command_id: u64, succeeded: bool) -> GuardEffect {
        if self.save_command_id != Some(command_id) {
            return GuardEffect::None;
        }
        self.save_command_id = None;
        let action = self.pending_action.take();
        match (succeeded, action) {
            (true, Some(action)) => GuardEffect::Continue(action),
            _ => GuardEffect::None,
        }
    }

    #[cfg(test)]
    fn is_pending(&self) -> bool {
        self.pending_action.is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{GuardChoice, GuardEffect, SessionAction, SessionGuard};

    #[test]
    fn close_request_is_accepted_only_when_clean() {
        let mut guard = SessionGuard::default();

        assert_eq!(guard.request_close(false), (true, GuardEffect::None));
        assert_eq!(guard.request_close(true), (false, GuardEffect::Prompt));
        assert!(guard.is_pending());
    }

    #[test]
    fn clean_action_continues_without_a_prompt() {
        let mut guard = SessionGuard::default();

        assert_eq!(
            guard.request(SessionAction::New, false),
            GuardEffect::Continue(SessionAction::New)
        );
        assert!(!guard.is_pending());
    }

    #[test]
    fn dirty_action_prompts_and_rejects_reentrant_requests() {
        let mut guard = SessionGuard::default();

        assert_eq!(
            guard.request(SessionAction::Open, true),
            GuardEffect::Prompt
        );
        assert_eq!(guard.request(SessionAction::New, true), GuardEffect::None);
        assert!(guard.is_pending());
    }

    #[test]
    fn discard_continues_and_cancel_clears_the_action() {
        let mut guard = SessionGuard::default();
        guard.request(SessionAction::NewFromTemplate, true);
        assert_eq!(
            guard.choose(GuardChoice::Discard, true),
            GuardEffect::Continue(SessionAction::NewFromTemplate)
        );
        assert!(!guard.is_pending());

        guard.request(SessionAction::Quit, true);
        assert_eq!(guard.choose(GuardChoice::Cancel, true), GuardEffect::None);
        assert!(!guard.is_pending());
    }

    #[test]
    fn titled_save_waits_for_matching_success_before_continuing() {
        let mut guard = SessionGuard::default();
        guard.request(SessionAction::Open, true);
        assert_eq!(
            guard.choose(GuardChoice::Save, true),
            GuardEffect::SaveCurrent
        );
        guard.save_started(42);
        assert_eq!(guard.command_finished(41, true), GuardEffect::None);
        assert!(guard.is_pending());
        assert_eq!(
            guard.command_finished(42, true),
            GuardEffect::Continue(SessionAction::Open)
        );
        assert!(!guard.is_pending());
    }

    #[test]
    fn untitled_save_requires_a_destination_and_cancellation_aborts() {
        let mut guard = SessionGuard::default();
        guard.request(SessionAction::New, true);
        assert_eq!(
            guard.choose(GuardChoice::Save, false),
            GuardEffect::ChooseSaveDestination
        );
        assert_eq!(guard.save_destination(None), GuardEffect::None);
        assert!(!guard.is_pending());
    }

    #[test]
    fn selected_destination_is_saved_before_continuing() {
        let mut guard = SessionGuard::default();
        guard.request(SessionAction::Quit, true);
        guard.choose(GuardChoice::Save, false);
        let path = PathBuf::from("show.ascs");
        assert_eq!(
            guard.save_destination(Some(path.clone())),
            GuardEffect::SaveTo(path)
        );
        guard.save_started(7);
        assert_eq!(
            guard.command_finished(7, true),
            GuardEffect::Continue(SessionAction::Quit)
        );
    }

    #[test]
    fn save_failure_aborts_the_original_action() {
        let mut guard = SessionGuard::default();
        guard.request(SessionAction::New, true);
        guard.choose(GuardChoice::Save, true);
        guard.save_started(9);

        assert_eq!(guard.command_finished(9, false), GuardEffect::None);
        assert!(!guard.is_pending());
    }
}

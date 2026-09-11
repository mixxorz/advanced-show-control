#[cfg(test)]
pub(crate) mod tests;

use std::sync::{Arc, Mutex};

use crate::{cue_lists::CueListDocument, scenes::SceneDocument};

#[derive(Debug, Clone, PartialEq)]
pub struct SessionDocument {
    pub scenes: SceneDocument,
    pub cue_lists: CueListDocument,
}

/// Serializes cancellation with the in-memory commit, including a late acknowledgement.
/// No external I/O or actor wait may run inside `commit`.
#[derive(Debug, Clone)]
pub struct SessionReplacement(Arc<Mutex<ReplacementState>>);

#[derive(Debug)]
enum ReplacementState {
    Pending(SessionDocument),
    Committed(SessionDocument),
    Canceled,
}

impl SessionReplacement {
    pub fn new(document: SessionDocument) -> Self {
        Self(Arc::new(Mutex::new(ReplacementState::Pending(document))))
    }

    /// @cc [owner:mixxorz,label:persistence;concurrency] replacement-single-commit
    /// For all clones of one ticket, `apply` MUST run at most once and only while the ticket is
    /// pending. A prior cancellation MUST prevent `apply`; after a commit, every later commit call
    /// MUST return the same committed document without applying its closure.
    /**
     * @cc [owner:mixxorz,label:persistence;concurrency] replacement-apply-must-remain-bounded
     * The synchronous `apply` callback MUST NOT perform external or blocking I/O or actor waits;
     * work inside it MUST remain bounded because it runs while the replacement lock serializes
     * commit against cancellation and late acknowledgement.
     */
    pub(crate) fn commit(
        &self,
        apply: impl FnOnce(SessionDocument) -> SessionDocument,
    ) -> Result<SessionDocument, String> {
        let mut state = self.0.lock().expect("session replacement lock poisoned");
        match std::mem::replace(&mut *state, ReplacementState::Canceled) {
            ReplacementState::Pending(document) => {
                let document = apply(document);
                *state = ReplacementState::Committed(document.clone());
                Ok(document)
            }
            ReplacementState::Committed(document) => {
                *state = ReplacementState::Committed(document.clone());
                Ok(document)
            }
            ReplacementState::Canceled => Err("Session replacement was canceled".into()),
        }
    }

    /// @cc [owner:mixxorz,label:persistence;concurrency] timeout-cancel-or-observe-commit
    /// This operation MUST serialize with `commit`: it MUST return the committed document when the
    /// commit won the race, otherwise permanently cancel the pending ticket so no later commit can
    /// mutate the session.
    pub(crate) fn cancel_or_committed(&self) -> Result<SessionDocument, String> {
        let mut state = self.0.lock().expect("session replacement lock poisoned");
        match &*state {
            ReplacementState::Committed(document) => Ok(document.clone()),
            _ => {
                *state = ReplacementState::Canceled;
                Err("Session replacement timed out before committing".into())
            }
        }
    }
}

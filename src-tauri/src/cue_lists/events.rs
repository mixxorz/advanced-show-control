use serde::Serialize;

use super::CueListDocument;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CueListsProjectionReason {
    CueListState,
    FileReplacement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CueListsProjectionState {
    pub document: CueListDocument,
    pub last_recall_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CueListsEvent {
    StateChanged {
        reason: CueListsProjectionReason,
        state: CueListsProjectionState,
        persisted_cue_list_edit: bool,
    },
}

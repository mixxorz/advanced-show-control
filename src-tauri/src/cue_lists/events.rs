use serde::Serialize;

use super::CueListDocument;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CueListsProjectionState {
    pub document: CueListDocument,
    pub last_recall_status: Option<String>,
}

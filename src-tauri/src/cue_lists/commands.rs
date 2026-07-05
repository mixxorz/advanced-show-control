use tokio::sync::oneshot;
use uuid::Uuid;

use super::{CueEntry, CueList, CueListDocument};

#[derive(Debug)]
pub enum CueListsCommand {
    InitialProjectionState {
        reply: oneshot::Sender<CueListsProjectionState>,
    },
    GetCueListDocument {
        reply: oneshot::Sender<CueListDocument>,
    },
    ReplaceCueListDocument {
        document: CueListDocument,
        persisted_cue_list_edit: bool,
        reply: Option<oneshot::Sender<CueListsCommandResult>>,
    },
    CreateCueList {
        name: String,
        reply: Option<oneshot::Sender<Result<CueListsCommandResult, String>>>,
    },
    RenameCueList {
        cue_list_id: Uuid,
        name: String,
        reply: Option<oneshot::Sender<Result<CueListsCommandResult, String>>>,
    },
    DeleteCueList {
        cue_list_id: Uuid,
        reply: Option<oneshot::Sender<Result<CueListsCommandResult, String>>>,
    },
    ReorderCueLists {
        ordered_ids: Vec<Uuid>,
        reply: Option<oneshot::Sender<Result<CueListsCommandResult, String>>>,
    },
    SetActiveCueList {
        cue_list_id: Option<Uuid>,
        reply: Option<oneshot::Sender<Result<CueListsCommandResult, String>>>,
    },
    AddSceneToActiveCueList {
        scene_internal_id: Uuid,
        insert_index: usize,
        reply: Option<oneshot::Sender<Result<CueListsCommandResult, String>>>,
    },
    RemoveCueEntry {
        cue_entry_id: Uuid,
        reply: Option<oneshot::Sender<Result<CueListsCommandResult, String>>>,
    },
    ReorderCueEntries {
        ordered_entry_ids: Vec<Uuid>,
        reply: Option<oneshot::Sender<Result<CueListsCommandResult, String>>>,
    },
    CueEntry {
        cue_entry_id: Option<Uuid>,
        reply: Option<oneshot::Sender<Result<CueListsCommandResult, String>>>,
    },
    RecallCuedCue {
        reply: oneshot::Sender<Result<CueRecallResult, crate::runtime::errors::AppCommandError>>,
    },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CueListsCommandResult {
    pub changed: bool,
    pub cue_list: Option<CueList>,
    pub entry: Option<CueEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CueRecallResult {
    pub recalled_entry_id: Uuid,
    pub next_cued_entry_id: Option<Uuid>,
}

pub use super::events::CueListsProjectionState;

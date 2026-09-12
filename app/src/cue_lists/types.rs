use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CueListDocument {
    pub cue_lists: Vec<CueList>,
    pub active_cue_list_id: Option<Uuid>,
    pub cued_cue_entry_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CueList {
    pub id: Uuid,
    pub name: String,
    pub entries: Vec<CueEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CueEntry {
    pub id: Uuid,
    pub scene_internal_id: Uuid,
}

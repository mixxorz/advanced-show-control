//! Show-owned application command handlers.

use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

pub enum ShowCommand {
    CurrentShowFilePath {
        reply: oneshot::Sender<Option<std::path::PathBuf>>,
    },
    GetLockout {
        reply: oneshot::Sender<bool>,
    },
    InitialProjectionState {
        reply: oneshot::Sender<super::events::ShowProjectionState>,
    },
    SetLockout {
        enabled: bool,
        reply: Option<oneshot::Sender<ShowCommandResult>>,
    },
    NewShowFileFromCurrentLv1 {
        reply: Option<oneshot::Sender<Result<NewShowFileResult, String>>>,
    },
    SaveShowFileAs {
        path: std::path::PathBuf,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    SetDiscoveredLv1Systems {
        systems: Vec<DiscoveredLv1System>,
        reply: Option<oneshot::Sender<ShowCommandResult>>,
    },
    CompleteLv1Connection {
        identity: Lv1SystemIdentity,
        reply: Option<oneshot::Sender<super::CompleteConnectionOutcome>>,
    },
    CompleteLv1ConnectionIfCurrent {
        identity: Lv1SystemIdentity,
        runtime_generation: crate::runtime::generation::RuntimeGeneration,
        expected_generation: u64,
        reply: oneshot::Sender<super::CompleteConnectionOutcome>,
    },
    ClearLv1ConnectionIfCurrent {
        runtime_generation: crate::runtime::generation::RuntimeGeneration,
        expected_generation: u64,
        reply: oneshot::Sender<super::CompleteConnectionOutcome>,
    },
    FailLv1Connection {
        reply: Option<oneshot::Sender<ShowCommandResult>>,
    },
    FailLv1ConnectionIfCurrent {
        runtime_generation: crate::runtime::generation::RuntimeGeneration,
        expected_generation: u64,
        reply: oneshot::Sender<super::CompleteConnectionOutcome>,
    },
    LoadShowFileFromPath {
        path: std::path::PathBuf,
        reply: Option<oneshot::Sender<Result<LoadShowFileResult, String>>>,
    },
    #[cfg(test)]
    ClearForTest {
        reply: Option<oneshot::Sender<()>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShowCommandResult {
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectCommandResult {
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewShowFileResult {
    pub selected_scene_internal_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadShowFileResult {
    pub selected_scene_internal_id: Option<String>,
    pub saved_at: String,
}

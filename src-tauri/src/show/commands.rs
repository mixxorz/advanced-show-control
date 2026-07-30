//! Show-owned application command handlers.

use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity};
use crate::show::show_file::LoadValidationReport;
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
    RefreshLv1Discovery {
        timeout_ms: Option<u64>,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    CompleteLv1Connection {
        identity: Lv1SystemIdentity,
        mode: super::ConnectionCompletionMode,
        reply: Option<oneshot::Sender<super::CompleteConnectionOutcome>>,
    },
    AuthorizeLv1ConnectionIfCurrent {
        mode: super::ConnectionCompletionMode,
        runtime_generation: crate::runtime::generation::RuntimeGeneration,
        expected_generation: u64,
        reply: oneshot::Sender<bool>,
    },
    CompleteLv1ConnectionIfCurrent {
        identity: Lv1SystemIdentity,
        mode: super::ConnectionCompletionMode,
        runtime_generation: crate::runtime::generation::RuntimeGeneration,
        expected_generation: u64,
        reply: oneshot::Sender<super::CompleteConnectionOutcome>,
    },
    FailLv1Connection {
        reply: Option<oneshot::Sender<ShowCommandResult>>,
    },
    FailLv1Reconnect {
        reply: Option<oneshot::Sender<ShowCommandResult>>,
    },
    FailLv1ConnectionIfCurrent {
        mode: super::ConnectionFailureMode,
        runtime_generation: crate::runtime::generation::RuntimeGeneration,
        expected_generation: u64,
        reply: oneshot::Sender<super::CompleteConnectionOutcome>,
    },
    ClaimReconnectTimeout {
        attempt: u64,
        reply: oneshot::Sender<bool>,
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
    #[serde(skip)]
    pub report: LoadValidationReport,
}

use serde::Serialize;
use tokio::sync::oneshot;

use crate::connection_state::Lv1SystemIdentity;
use crate::runtime::generation::RuntimeGeneration;

use super::AppSettings;

#[derive(Debug)]
pub enum SettingsCommand {
    GetSettings {
        reply: oneshot::Sender<AppSettings>,
    },
    ReplaceSettings {
        settings: AppSettings,
        reply: oneshot::Sender<Result<SettingsCommandResult, String>>,
    },
    GetLastConnectedLv1 {
        reply: oneshot::Sender<Option<Lv1SystemIdentity>>,
    },
    SetLastConnectedLv1 {
        identity: Lv1SystemIdentity,
        runtime_generation: RuntimeGeneration,
        expected_generation: u64,
        reply: oneshot::Sender<Result<SettingsCommandResult, String>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettingsCommandResult {
    pub changed: bool,
}

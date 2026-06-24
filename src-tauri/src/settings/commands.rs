use serde::Serialize;
use tokio::sync::oneshot;

use super::AppSettings;

#[derive(Debug)]
pub enum SettingsCommand {
    GetSettings {
        reply: oneshot::Sender<AppSettings>,
    },
    ReplaceSettings {
        settings: AppSettings,
        reply: Option<oneshot::Sender<Result<SettingsCommandResult, String>>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettingsCommandResult {
    pub changed: bool,
}

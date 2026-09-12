use std::path::PathBuf;

use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity};

#[derive(Debug, Clone, PartialEq)]
pub struct ShowProjectionState {
    pub lockout: bool,
    pub show_file_path: Option<PathBuf>,
    pub show_file_name: String,
    pub show_file_dirty: bool,
    pub show_file_last_saved_at: Option<String>,
    pub discovered_lv1_systems: Vec<DiscoveredLv1System>,
    pub connected_lv1_identity: Option<Lv1SystemIdentity>,
    pub last_event_at: Option<String>,
}

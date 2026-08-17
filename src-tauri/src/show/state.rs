use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompleteConnectionOutcome {
    pub accepted: bool,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShowState {
    lockout: bool,
    show_file_path: Option<std::path::PathBuf>,
    show_file_dirty: bool,
    show_file_last_saved_at: Option<String>,
    discovered_lv1_systems: Vec<DiscoveredLv1System>,
    connected_lv1_identity: Option<Lv1SystemIdentity>,
    last_event_at: Option<String>,
}

impl ShowState {
    pub(crate) fn reset_for_new_show(&mut self) {
        self.clear();
        self.show_file_path = None;
        self.show_file_dirty = false;
        self.show_file_last_saved_at = None;
    }

    pub(crate) fn mark_saved(&mut self, path: std::path::PathBuf, saved_at: String) {
        self.show_file_path = Some(path);
        self.show_file_last_saved_at = Some(saved_at);
        self.show_file_dirty = false;
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.show_file_dirty = true;
    }

    pub(crate) fn set_discovered_lv1_systems(&mut self, systems: Vec<DiscoveredLv1System>) -> bool {
        if self.discovered_lv1_systems == systems {
            false
        } else {
            self.discovered_lv1_systems = systems;
            true
        }
    }

    pub(crate) fn complete_lv1_connection(
        &mut self,
        identity: Lv1SystemIdentity,
    ) -> CompleteConnectionOutcome {
        let changed = self.connected_lv1_identity.as_ref() != Some(&identity);
        self.connected_lv1_identity = Some(identity);
        CompleteConnectionOutcome {
            accepted: true,
            changed,
        }
    }

    pub(crate) fn clear_lv1_connection(&mut self) -> bool {
        let changed = self.connected_lv1_identity.take().is_some();
        if changed {
            self.last_event_at = Some(crate::time::current_timestamp_millis());
        }
        changed
    }

    pub(crate) fn fail_lv1_connection(&mut self) -> bool {
        self.clear_lv1_connection()
    }

    pub(crate) fn lockout(&self) -> bool {
        self.lockout
    }

    pub(crate) fn current_show_file_path(&self) -> Option<std::path::PathBuf> {
        self.show_file_path.clone()
    }

    pub fn projection_state(&self) -> super::events::ShowProjectionState {
        let show_file_name = self
            .show_file_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| "Untitled Session".to_string());

        super::events::ShowProjectionState {
            lockout: self.lockout,
            show_file_path: self.show_file_path.clone(),
            show_file_name,
            show_file_dirty: self.show_file_dirty,
            show_file_last_saved_at: self.show_file_last_saved_at.clone(),
            discovered_lv1_systems: self.discovered_lv1_systems.clone(),
            connected_lv1_identity: self.connected_lv1_identity.clone(),
            last_event_at: self.last_event_at.clone(),
        }
    }

    pub fn clear(&mut self) {
        self.lockout = false;
    }

    pub fn set_lockout(&mut self, enabled: bool) -> bool {
        if self.lockout == enabled {
            false
        } else {
            self.lockout = enabled;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(uuid: &str) -> Lv1SystemIdentity {
        Lv1SystemIdentity {
            uuid: Some(uuid.to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50_000,
        }
    }

    #[test]
    fn unconditional_completion_sets_identity_and_reports_changes() {
        let next = identity("new");
        let mut state = ShowState::default();

        assert_eq!(
            state.complete_lv1_connection(next.clone()),
            CompleteConnectionOutcome {
                accepted: true,
                changed: true,
            }
        );
        assert_eq!(state.projection_state().connected_lv1_identity, Some(next));
        assert!(!state.complete_lv1_connection(identity("new")).changed);
    }

    #[test]
    fn failed_connection_clears_identity() {
        let mut state = ShowState {
            connected_lv1_identity: Some(identity("old")),
            ..Default::default()
        };

        assert!(state.fail_lv1_connection());
        assert_eq!(state.projection_state().connected_lv1_identity, None);
        assert!(!state.fail_lv1_connection());
    }
}

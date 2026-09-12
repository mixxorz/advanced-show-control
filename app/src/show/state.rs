use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompleteConnectionOutcome {
    pub accepted: bool,
    pub changed: bool,
}

/// @cc [owner:mixxorz,label:architecture] show-state-owns-session-metadata-only
/// `ShowState` MUST own only lockout, show-file metadata/dirty state, discovery results, and
/// connected-LV1 metadata; scene configurations, selection, clipboard, and cue documents MUST remain
/// owned by the Scenes domain.
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
    /// @cc [owner:mixxorz,label:persistence] new-show-metadata-reset
    /// After documents for a new show have committed, resetting metadata MUST clear lockout, path,
    /// saved timestamp, and dirty state without altering discovery or connected-LV1 metadata.
    pub(crate) fn reset_for_new_show(&mut self) {
        self.clear();
        self.show_file_path = None;
        self.show_file_dirty = false;
        self.show_file_last_saved_at = None;
    }

    /// @cc [owner:mixxorz,label:persistence] saved-metadata-is-clean
    /// Recording a successful save or load MUST atomically adopt its path and saved timestamp and
    /// clear dirty state; callers MUST re-mark dirty afterward when import normalization changed the
    /// persisted document.
    pub(crate) fn mark_saved(&mut self, path: std::path::PathBuf, saved_at: String) {
        self.show_file_path = Some(path);
        self.show_file_last_saved_at = Some(saved_at);
        self.show_file_dirty = false;
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.show_file_dirty = true;
    }

    /// @cc [owner:mixxorz,label:product] discovery-is-whole-list-state
    /// A discovery update MUST replace the complete discovered-system list and report `changed`
    /// exactly when the ordered list differs; it MUST NOT modify connection identity or file state.
    pub(crate) fn set_discovered_lv1_systems(&mut self, systems: Vec<DiscoveredLv1System>) -> bool {
        if self.discovered_lv1_systems == systems {
            false
        } else {
            self.discovered_lv1_systems = systems;
            true
        }
    }

    /// @cc [owner:mixxorz,label:product] connection-metadata-transition
    /// Setting connection metadata MUST report whether identity changed. A transition to no identity
    /// MUST timestamp `last_event_at`; an accepted no-op or transition to an identity MUST NOT
    /// overwrite that timestamp.
    pub(crate) fn set_lv1_connection(&mut self, identity: Option<Lv1SystemIdentity>) -> bool {
        let changed = self.connected_lv1_identity != identity;
        if changed && identity.is_none() {
            self.last_event_at = Some(crate::time::current_timestamp_millis());
        }
        self.connected_lv1_identity = identity;
        changed
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

    /// @cc [owner:mixxorz,label:safety] lockout-change-outcome
    /// Lockout updates MUST report `true` only when the stored safety value changes; repeated values
    /// MUST remain no-ops so callers do not publish misleading state changes.
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

        assert!(state.set_lv1_connection(Some(next.clone())));
        assert_eq!(state.projection_state().connected_lv1_identity, Some(next));
        assert!(!state.set_lv1_connection(Some(identity("new"))));
    }

    #[test]
    fn failed_connection_clears_identity() {
        let mut state = ShowState {
            connected_lv1_identity: Some(identity("old")),
            ..Default::default()
        };

        assert!(state.set_lv1_connection(None));
        assert_eq!(state.projection_state().connected_lv1_identity, None);
        assert!(!state.set_lv1_connection(None));
    }
}

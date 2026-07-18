use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShowState {
    lockout: bool,
    show_file_path: Option<std::path::PathBuf>,
    show_file_dirty: bool,
    show_file_last_saved_at: Option<String>,
    discovered_lv1_systems: Vec<DiscoveredLv1System>,
    connected_lv1_identity: Option<Lv1SystemIdentity>,
    pending_lv1_identity: Option<Lv1SystemIdentity>,
    reconnect: ReconnectState,
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

    pub(crate) fn complete_lv1_connection(&mut self, identity: Lv1SystemIdentity) -> bool {
        let reconnect = ReconnectState::default();
        let changed = self.connected_lv1_identity.as_ref() != Some(&identity)
            || self.pending_lv1_identity.is_some()
            || self.reconnect != reconnect;
        self.connected_lv1_identity = Some(identity);
        self.pending_lv1_identity = None;
        self.reconnect = reconnect;
        changed
    }

    pub(crate) fn fail_lv1_connection(&mut self) -> bool {
        let reconnect = ReconnectState::default();
        let changed = self.connected_lv1_identity.is_some()
            || self.pending_lv1_identity.is_some()
            || self.reconnect != reconnect;
        self.connected_lv1_identity = None;
        self.pending_lv1_identity = None;
        self.reconnect = reconnect;
        changed
    }

    pub(crate) fn fail_lv1_reconnect(&mut self) -> bool {
        let reconnect = ReconnectState::default();
        let changed = self.pending_lv1_identity.is_some() || self.reconnect != reconnect;
        self.pending_lv1_identity = None;
        self.reconnect = reconnect;
        changed
    }

    #[cfg(test)]
    pub(crate) fn with_connection_metadata_for_test(
        connected_lv1_identity: Lv1SystemIdentity,
        pending_lv1_identity: Option<Lv1SystemIdentity>,
        reconnect: ReconnectState,
        last_event_at: Option<String>,
    ) -> Self {
        Self {
            connected_lv1_identity: Some(connected_lv1_identity),
            pending_lv1_identity,
            reconnect,
            last_event_at,
            ..Default::default()
        }
    }

    pub(crate) fn handle_runtime_disconnected(&mut self, _reason: String) -> bool {
        let mut changed = false;
        if self.connected_lv1_identity.take().is_some() {
            changed = true;
        }
        if self.pending_lv1_identity.take().is_some() {
            changed = true;
        }
        let next = ReconnectState {
            active: false,
            attempt: 0,
        };
        if self.reconnect != next {
            self.reconnect = next;
            changed = true;
        }
        let timestamp = crate::time::current_timestamp_millis();
        if self.last_event_at.as_ref() != Some(&timestamp) {
            self.last_event_at = Some(timestamp);
            changed = true;
        }
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
            pending_lv1_identity: self.pending_lv1_identity.clone(),
            reconnect: self.reconnect.clone(),
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
    fn complete_connection_sets_identity_and_clears_transient_metadata_atomically() {
        let next = identity("new");
        let mut state = ShowState {
            connected_lv1_identity: Some(identity("old")),
            pending_lv1_identity: Some(next.clone()),
            reconnect: ReconnectState {
                active: true,
                attempt: 3,
            },
            ..Default::default()
        };

        assert!(state.complete_lv1_connection(next.clone()));
        let projection = state.projection_state();
        assert_eq!(projection.connected_lv1_identity, Some(next.clone()));
        assert_eq!(projection.pending_lv1_identity, None);
        assert_eq!(projection.reconnect, ReconnectState::default());
        assert!(!state.complete_lv1_connection(next));
    }

    #[test]
    fn failed_connection_clears_all_connection_metadata() {
        let mut state = ShowState {
            connected_lv1_identity: Some(identity("old")),
            pending_lv1_identity: Some(identity("new")),
            reconnect: ReconnectState {
                active: true,
                attempt: 2,
            },
            ..Default::default()
        };

        assert!(state.fail_lv1_connection());
        let projection = state.projection_state();
        assert_eq!(projection.connected_lv1_identity, None);
        assert_eq!(projection.pending_lv1_identity, None);
        assert_eq!(projection.reconnect, ReconnectState::default());
        assert!(!state.fail_lv1_connection());
    }

    #[test]
    fn failed_reconnect_preserves_connected_identity_and_clears_transient_metadata() {
        let connected = identity("old");
        let mut state = ShowState {
            connected_lv1_identity: Some(connected.clone()),
            pending_lv1_identity: Some(identity("new")),
            reconnect: ReconnectState {
                active: true,
                attempt: 4,
            },
            last_event_at: Some("2026-07-19T12:00:00.000Z".to_string()),
            ..Default::default()
        };

        assert!(state.fail_lv1_reconnect());
        let projection = state.projection_state();
        assert_eq!(projection.connected_lv1_identity, Some(connected));
        assert_eq!(projection.pending_lv1_identity, None);
        assert_eq!(projection.reconnect, ReconnectState::default());
        assert_eq!(
            projection.last_event_at.as_deref(),
            Some("2026-07-19T12:00:00.000Z")
        );
        assert!(!state.fail_lv1_reconnect());
    }
}

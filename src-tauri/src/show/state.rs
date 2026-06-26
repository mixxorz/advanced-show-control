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

    pub(crate) fn set_pending_lv1_identity(&mut self, identity: Option<Lv1SystemIdentity>) -> bool {
        if self.pending_lv1_identity == identity {
            false
        } else {
            self.pending_lv1_identity = identity;
            true
        }
    }

    pub(crate) fn establish_connected_lv1_identity(&mut self, identity: Lv1SystemIdentity) -> bool {
        let changed = self.connected_lv1_identity.as_ref() != Some(&identity)
            || self.pending_lv1_identity.is_some();
        if changed {
            self.connected_lv1_identity = Some(identity);
            self.pending_lv1_identity = None;
        }
        changed
    }

    pub(crate) fn clear_connected_lv1_identity(&mut self) -> bool {
        if self.connected_lv1_identity.is_none() {
            false
        } else {
            self.connected_lv1_identity = None;
            true
        }
    }

    pub(crate) fn set_reconnect_state(&mut self, reconnect: ReconnectState) -> bool {
        if self.reconnect == reconnect {
            false
        } else {
            self.reconnect = reconnect;
            true
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
mod tests {}

use crate::cue_lists::CueListsProjectionState;
use crate::scenes::{ScenesEvent, ScenesProjectionState};
use crate::settings::{AppSettings, SettingsEvent};
use crate::show::{ShowProjectionState, ShowState};

use super::events::AppEvent;

/// @cc [owner:mixxorz,label:architecture] retained-app-state-only
/// The retained snapshot MUST contain only app-lifetime Show, Scenes, Cue Lists, and Settings
/// projections; generation-bound LV1/Fade state and operational runtime facts MUST NOT be retained
/// here.
#[derive(Debug, Clone, PartialEq)]
pub struct AppStateSnapshot {
    pub show: ShowProjectionState,
    pub scenes: ScenesProjectionState,
    pub cue_lists: CueListsProjectionState,
    pub session_revision: u64,
    pub settings: AppSettings,
}

impl Default for AppStateSnapshot {
    fn default() -> Self {
        Self {
            show: ShowState::default().projection_state(),
            scenes: ScenesProjectionState::default(),
            cue_lists: CueListsProjectionState::default(),
            session_revision: 0,
            settings: AppSettings::default(),
        }
    }
}

impl AppStateSnapshot {
    /**
     * @cc [owner:mixxorz,label:architecture] retained-event-selection
     * Applying an event MUST update only the retained projection family represented by `Show`,
     * scene `StateChanged`, `CueLists`, settings `StateChanged`, or `SessionReplaced`; all other
     * facts MUST leave the snapshot unchanged and return `false`.
     */
    /**
     * @cc [owner:mixxorz,label:consistency] session-replacement-atomic-projection
     * `SessionReplaced` MUST apply its Scenes and Cue Lists projections in the same watch-state
     * mutation and increment `session_revision`, preventing a mixed retained session snapshot and
     * allowing presentation-only work from the prior session to be discarded.
     */
    pub(super) fn apply(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::Show(state) => replace(&mut self.show, state),
            AppEvent::Scenes {
                event: ScenesEvent::StateChanged { state, .. },
                ..
            } => replace(&mut self.scenes, state),
            AppEvent::CueLists(state) => replace(&mut self.cue_lists, state),
            AppEvent::Settings(SettingsEvent::StateChanged { settings }) => {
                replace(&mut self.settings, settings)
            }
            AppEvent::SessionReplaced {
                scenes, cue_lists, ..
            } => {
                replace(&mut self.scenes, scenes);
                replace(&mut self.cue_lists, cue_lists);
                self.session_revision = self.session_revision.saturating_add(1);
                true
            }
            _ => false,
        }
    }
}

fn replace<T: Clone + PartialEq>(current: &mut T, next: &T) -> bool {
    if current == next {
        false
    } else {
        *current = next.clone();
        true
    }
}

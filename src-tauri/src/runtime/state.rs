use crate::cue_lists::CueListsProjectionState;
use crate::scenes::{ScenesEvent, ScenesProjectionState};
use crate::settings::{AppSettings, SettingsEvent};
use crate::show::{ShowProjectionState, ShowState};

use super::events::AppEvent;

#[derive(Debug, Clone, PartialEq)]
pub struct AppStateSnapshot {
    pub show: ShowProjectionState,
    pub scenes: ScenesProjectionState,
    pub cue_lists: CueListsProjectionState,
    pub settings: AppSettings,
}

impl Default for AppStateSnapshot {
    fn default() -> Self {
        Self {
            show: ShowState::default().projection_state(),
            scenes: ScenesProjectionState::default(),
            cue_lists: CueListsProjectionState::default(),
            settings: AppSettings::default(),
        }
    }
}

impl AppStateSnapshot {
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
                let scenes_changed = replace(&mut self.scenes, scenes);
                let cues_changed = replace(&mut self.cue_lists, cue_lists);
                scenes_changed || cues_changed
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

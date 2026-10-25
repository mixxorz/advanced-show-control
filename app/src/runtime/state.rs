use crate::cue_lists::CueListsProjectionState;
use crate::fade::FadeEvent;
use crate::lv1::Lv1Event;
use crate::projector::{AppConnectionState, AppFadeState, ChannelSummary, SceneSummary};
use crate::scenes::{ScenesEvent, ScenesProjectionState};
use crate::settings::{AppSettings, SettingsEvent};
use crate::show::{ShowProjectionState, ShowState};

use super::events::{AppEvent, RuntimeLifecycleEvent};

/// @cc [owner:mixxorz,label:architecture] retained-app-state-only
/// The retained snapshot MUST contain app-lifetime projections and the latest generation-tagged
/// LV1/Fade display state. Operational parameter values, readiness and control state MUST remain
/// with their domain owners; display retention MUST NOT replace their operational event delivery.
#[derive(Debug, Clone, PartialEq)]
pub struct AppStateSnapshot {
    pub display: RuntimeDisplayState,
    pub show: ShowProjectionState,
    pub scenes: ScenesProjectionState,
    pub cue_lists: CueListsProjectionState,
    pub settings: AppSettings,
    pub updates: crate::updates::UpdateState,
}

impl Default for AppStateSnapshot {
    fn default() -> Self {
        Self {
            display: RuntimeDisplayState::default(),
            show: ShowState::default().projection_state(),
            scenes: ScenesProjectionState::default(),
            cue_lists: CueListsProjectionState::default(),
            settings: AppSettings::default(),
            updates: crate::updates::UpdateState::default(),
        }
    }
}

/// @cc [owner:mixxorz,label:architecture;generation] projector-event-routing
/// Runtime generation changes MUST clear the retained live display. LV1/Fade display facts MUST
/// match that generation; app-lifetime documents MUST remain independent of it.
/// @cc [owner:mixxorz,label:generation;state] generation-reset-boundary
/// A generation reset MUST clear LV1 display data and return Fade to Idle, preserving app-lifetime
/// projections and projector-owned logs and snapshot/log counters.
/// @cc [owner:mixxorz,label:generation;projection] lv1-event-materiality
/// Stale-generation LV1 facts and parameter/keepalive facts MUST NOT change retained display.
/// Connection, scene, scene-list, topology and disconnect facts MUST retain only latest display
/// values; unchanged values MUST NOT notify watchers.
/// @cc [owner:mixxorz,label:generation;projection] fade-event-generation-filter
/// Stale-generation Fade facts MUST NOT change retained display. Current-generation aggregate
/// Running/Blocked/Idle changes MUST be retained; per-channel facts without aggregate changes MUST
/// NOT notify display watchers.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeDisplayState {
    pub generation: u64,
    pub connection: AppConnectionState,
    pub current_scene: Option<SceneSummary>,
    pub scenes: Vec<SceneSummary>,
    pub channels: Vec<ChannelSummary>,
    pub fade_state: AppFadeState,
}

impl Default for RuntimeDisplayState {
    fn default() -> Self {
        Self {
            generation: 0,
            connection: AppConnectionState::Disconnected,
            current_scene: None,
            scenes: Vec::new(),
            channels: Vec::new(),
            fade_state: AppFadeState::Idle,
        }
    }
}

impl AppStateSnapshot {
    /**
     * @cc [owner:mixxorz,label:architecture] retained-event-selection
     * Applying an event MUST update only its retained app-lifetime projection or current-generation
     * display fields. Parameter, keepalive and other non-display operational facts MUST leave the
     * snapshot unchanged and return `false`. Duplicate display values MUST NOT notify watchers.
     */
    /**
     * @cc [owner:mixxorz,label:consistency] session-replacement-atomic-projection
     * `SessionReplaced` MUST apply its Scenes and Cue Lists projections in the same watch-state
     * mutation, preventing a mixed retained session snapshot.
     */
    /**
     * @cc [owner:mixxorz,label:safety;updates] retained-update-channel-revision
     * Retained Settings MUST expose the current update channel before notification. Intervening
     * channel toggles need no historical revision and MUST NOT advance the session revision.
     */
    pub(super) fn apply(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation }) => {
                if self.display.generation == *generation {
                    return false;
                }
                self.display = RuntimeDisplayState {
                    generation: *generation,
                    ..Default::default()
                };
                true
            }
            AppEvent::Lv1 { generation, event } if *generation == self.display.generation => {
                match event {
                    Lv1Event::Connected => {
                        replace_owned(&mut self.display.connection, AppConnectionState::Connected)
                    }
                    Lv1Event::Disconnected { .. } => {
                        let disconnected = RuntimeDisplayState {
                            generation: *generation,
                            fade_state: self.display.fade_state.clone(),
                            ..Default::default()
                        };
                        replace_owned(&mut self.display, disconnected)
                    }
                    Lv1Event::SceneChanged(observation) => replace_owned(
                        &mut self.display.current_scene,
                        Some(SceneSummary {
                            index: observation.scene.index,
                            name: observation.scene.name.clone(),
                        }),
                    ),
                    Lv1Event::SceneListChanged(scenes) => replace_owned(
                        &mut self.display.scenes,
                        scenes
                            .iter()
                            .map(|scene| SceneSummary {
                                index: scene.index,
                                name: scene.name.clone(),
                            })
                            .collect(),
                    ),
                    Lv1Event::ChannelTopologyChanged(channels) => replace_owned(
                        &mut self.display.channels,
                        channels
                            .iter()
                            .map(|channel| ChannelSummary {
                                group: channel.group,
                                channel: channel.channel,
                                name: channel.name.clone(),
                            })
                            .collect(),
                    ),
                    _ => false,
                }
            }
            AppEvent::Fade { generation, event } if *generation == self.display.generation => {
                let fade = match event {
                    FadeEvent::FadeStarted => AppFadeState::Running,
                    FadeEvent::FadeCompleted | FadeEvent::FadeAborted => AppFadeState::Idle,
                    FadeEvent::ChannelOverride { .. } => AppFadeState::Blocked,
                    _ => return false,
                };
                replace_owned(&mut self.display.fade_state, fade)
            }
            AppEvent::Show(state) => replace(&mut self.show, state),
            AppEvent::Updates(state) => replace(&mut self.updates, state),
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

fn replace_owned<T: PartialEq>(current: &mut T, next: T) -> bool {
    if *current == next {
        false
    } else {
        *current = next;
        true
    }
}

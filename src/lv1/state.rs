//! LV1 live state mirror helpers.

use std::time::Instant;

use crate::osc::OscArg;
use crate::runtime::events::{AppEvent, AppEventBus};

use super::events::Lv1Event;
use super::parsers::{parse_channels_batch, parse_scene_list};
use super::types::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry, SceneState};

// ---------------------------------------------------------------------------
// Group constants (confirmed from hardware logs)
// ---------------------------------------------------------------------------

pub mod group {
    pub const INPUT: i32 = 0;
    pub const GROUP: i32 = 1;
    pub const AUX: i32 = 2;
    pub const LR: i32 = 3;
    pub const CENTER: i32 = 4;
    pub const MONO: i32 = 5;
    pub const MATRIX: i32 = 6;
    pub const CUE: i32 = 7;
    pub const TALKBACK: i32 = 8;
}

/// Pairs `/Notify/CurSceneIndex` and `/Notify/Scene/Name` OSC messages into a
/// complete `SceneState`. LV1 sends these as two separate messages that always
/// arrive close together but in either order. Call `apply_index` and `apply_name`
/// as messages arrive; the buffer emits `Some(SceneState)` once both have been
/// received, then clears itself.
#[derive(Default)]
pub(super) struct SceneBuffer {
    pending_index: Option<i32>,
    pending_name: Option<String>,
}

impl SceneBuffer {
    pub(super) fn apply_index(&mut self, index: i32) -> Option<SceneState> {
        self.pending_index = Some(index);
        self.try_emit()
    }

    pub(super) fn apply_name(&mut self, name: String) -> Option<SceneState> {
        self.pending_name = Some(name);
        self.try_emit()
    }

    fn try_emit(&mut self) -> Option<SceneState> {
        if self.pending_index.is_some() && self.pending_name.is_some() {
            let index = self.pending_index.take().unwrap();
            let name = self.pending_name.take().unwrap();
            Some(SceneState { index, name })
        } else {
            None
        }
    }
}

pub(super) fn apply_fader_update(channels: &mut Vec<ChannelInfo>, group: i32, channel: i32, gain_db: f64) {
    if let Some(ch) = channels
        .iter_mut()
        .find(|c| c.group == group && c.channel == channel)
    {
        ch.gain_db = gain_db;
    }
}

pub(super) fn apply_mute_update(channels: &mut Vec<ChannelInfo>, group: i32, channel: i32, muted: bool) {
    if let Some(ch) = channels
        .iter_mut()
        .find(|c| c.group == group && c.channel == channel)
    {
        ch.muted = muted;
    }
}

pub(super) fn osc_arg_to_bool(arg: &OscArg) -> Option<bool> {
    match arg {
        OscArg::Bool(value) => Some(*value),
        OscArg::True => Some(true),
        OscArg::False => Some(false),
        OscArg::Int(0) => Some(false),
        OscArg::Int(1) => Some(true),
        _ => None,
    }
}

pub(super) struct ActorState {
    pub(super) connection: ConnectionStatus,
    pub(super) scene: Option<SceneState>,
    pub(super) scene_list: Vec<SceneListEntry>,
    pub(super) channels: Vec<ChannelInfo>,
    pub(super) scene_buf: SceneBuffer,
    pub(super) last_ping: Instant,
    pub(super) event_bus: AppEventBus,
}

impl ActorState {
    pub(super) fn new(event_bus: AppEventBus) -> Self {
        Self {
            connection: ConnectionStatus::Connecting,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
            scene_buf: SceneBuffer::default(),
            last_ping: Instant::now(),
            event_bus,
        }
    }

    pub(super) fn snapshot(&self) -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: self.connection.clone(),
            scene: self.scene.clone(),
            scene_list: self.scene_list.clone(),
            channels: self.channels.clone(),
        }
    }

    pub(super) fn fan_out(&mut self, event: Lv1Event) {
        self.event_bus.publish(AppEvent::Lv1(event));
    }
}

pub(super) fn handle_message(state: &mut ActorState, msg: &crate::osc::OscMessage) {
    match msg.address.as_str() {
        "/Channels" => {
            if let Ok(channels) = parse_channels_batch(&msg.args) {
                state.channels = channels.clone();
                state.fan_out(Lv1Event::ChannelTopologyChanged(channels));
            }
        }
        "/Notify/CurSceneIndex" => {
            if let Some(crate::osc::OscArg::Int(index)) = msg.args.first() {
                if let Some(scene) = state.scene_buf.apply_index(*index) {
                    state.scene = Some(scene.clone());
                    state.fan_out(Lv1Event::SceneChanged(scene));
                }
            }
        }
        "/Notify/Scene/Name" => {
            if let Some(crate::osc::OscArg::String(name)) = msg.args.first() {
                if let Some(scene) = state.scene_buf.apply_name(name.clone()) {
                    state.scene = Some(scene.clone());
                    state.fan_out(Lv1Event::SceneChanged(scene));
                }
            }
        }
        "/Notify/SceneList" => {
            if let Ok(list) = parse_scene_list(&msg.args) {
                state.scene_list = list.clone();
                state.fan_out(Lv1Event::SceneListChanged(list));
            }
        }
        "/Notify/Track/Out/Gain" => {
            if let (
                Some(crate::osc::OscArg::Int(group)),
                Some(crate::osc::OscArg::Int(channel)),
                Some(crate::osc::OscArg::Double(gain_db)),
            ) = (msg.args.first(), msg.args.get(1), msg.args.get(2))
            {
                apply_fader_update(&mut state.channels, *group, *channel, *gain_db);
                state.fan_out(Lv1Event::FaderChanged {
                    group: *group,
                    channel: *channel,
                    gain_db: *gain_db,
                });
            }
        }
        "/Notify/Track/Out/Mute" => {
            if let (
                Some(crate::osc::OscArg::Int(group)),
                Some(crate::osc::OscArg::Int(channel)),
                Some(mute_arg),
            ) = (msg.args.first(), msg.args.get(1), msg.args.get(2))
            {
                if let Some(muted) = osc_arg_to_bool(mute_arg) {
                    apply_mute_update(&mut state.channels, *group, *channel, muted);
                    state.fan_out(Lv1Event::MuteChanged {
                        group: *group,
                        channel: *channel,
                        muted,
                    });
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn actor_publishes_scene_changes_to_event_bus() {
        use crate::lv1::events::Lv1Event;
        use crate::runtime::events::{AppEvent, AppEventBus};

        let bus = AppEventBus::new(16);
        let mut rx = bus.subscribe();
        let mut state = ActorState::new(bus.clone());

        handle_message(
            &mut state,
            &crate::osc::OscMessage {
                address: "/Notify/CurSceneIndex".to_string(),
                args: vec![crate::osc::OscArg::Int(3)],
            },
        );
        handle_message(
            &mut state,
            &crate::osc::OscMessage {
                address: "/Notify/Scene/Name".to_string(),
                args: vec![crate::osc::OscArg::String("Bridge".to_string())],
            },
        );

        let event = rx.recv().await.unwrap();
        match event {
            AppEvent::Lv1(Lv1Event::SceneChanged(scene)) => {
                assert_eq!(scene.index, 3);
                assert_eq!(scene.name, "Bridge");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn scene_buffer_emits_when_name_arrives_first() {
        let mut buf = SceneBuffer::default();
        assert!(buf.apply_name("Scene A".to_string()).is_none());
        let scene = buf.apply_index(0).unwrap();
        assert_eq!(
            scene,
            SceneState {
                index: 0,
                name: "Scene A".to_string()
            }
        );
        assert!(buf.apply_index(0).is_none());
    }

    #[test]
    fn scene_buffer_emits_when_index_arrives_first() {
        let mut buf = SceneBuffer::default();
        assert!(buf.apply_index(1).is_none());
        let scene = buf.apply_name("Scene B".to_string()).unwrap();
        assert_eq!(
            scene,
            SceneState {
                index: 1,
                name: "Scene B".to_string()
            }
        );
    }

    #[test]
    fn scene_buffer_overwrites_pending_with_new_name() {
        let mut buf = SceneBuffer::default();
        buf.apply_name("Old".to_string());
        buf.apply_name("New".to_string());
        let scene = buf.apply_index(2).unwrap();
        assert_eq!(scene.name, "New");
    }

    #[test]
    fn apply_fader_update_changes_matching_channel() {
        let mut channels = vec![
            ChannelInfo {
                group: 0,
                channel: 0,
                name: "Ch 1".to_string(),
                gain_db: -9.0,
                muted: false,
            },
            ChannelInfo {
                group: 0,
                channel: 1,
                name: "Ch 2".to_string(),
                gain_db: -12.0,
                muted: false,
            },
        ];
        apply_fader_update(&mut channels, 0, 0, -6.0);
        assert_eq!(channels[0].gain_db, -6.0);
        assert_eq!(channels[1].gain_db, -12.0);
    }

    #[test]
    fn apply_fader_update_ignores_unknown_channel() {
        let mut channels = vec![ChannelInfo {
            group: 0,
            channel: 0,
            name: "Ch 1".to_string(),
            gain_db: -9.0,
            muted: false,
        }];
        apply_fader_update(&mut channels, 0, 99, -3.0);
        assert_eq!(channels[0].gain_db, -9.0);
    }

    #[test]
    fn apply_mute_update_changes_matching_channel() {
        let mut channels = vec![
            ChannelInfo {
                group: 0,
                channel: 0,
                name: "Ch 1".to_string(),
                gain_db: -9.0,
                muted: false,
            },
            ChannelInfo {
                group: 0,
                channel: 1,
                name: "Ch 2".to_string(),
                gain_db: -12.0,
                muted: false,
            },
        ];
        apply_mute_update(&mut channels, 0, 0, true);
        assert!(channels[0].muted);
        assert!(!channels[1].muted);
    }

    #[test]
    fn apply_mute_update_ignores_unknown_channel() {
        let mut channels = vec![ChannelInfo {
            group: 0,
            channel: 0,
            name: "Ch 1".to_string(),
            gain_db: -9.0,
            muted: false,
        }];
        apply_mute_update(&mut channels, 0, 99, true);
        assert!(!channels[0].muted);
    }

    #[test]
    fn osc_bool_values_map_to_mute_state() {
        assert_eq!(osc_arg_to_bool(&OscArg::Bool(true)), Some(true));
        assert_eq!(osc_arg_to_bool(&OscArg::Bool(false)), Some(false));
        assert_eq!(osc_arg_to_bool(&OscArg::Int(1)), Some(true));
        assert_eq!(osc_arg_to_bool(&OscArg::Int(0)), Some(false));
        assert_eq!(osc_arg_to_bool(&OscArg::Int(2)), None);
    }
}

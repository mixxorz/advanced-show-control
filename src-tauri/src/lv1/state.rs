//! LV1 live state mirror helpers.

use std::time::Instant;

use crate::runtime::events::AppEventBus;

use super::events::Lv1Event;
use super::parsers::{parse_channels_batch, parse_scene_list};
use super::types::{
    ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry, SceneObservation, SceneState,
};

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

pub(super) fn apply_fader_update(
    channels: &mut [ChannelInfo],
    group: i32,
    channel: i32,
    gain_db: f64,
) {
    if let Some(ch) = channels
        .iter_mut()
        .find(|c| c.group == group && c.channel == channel)
    {
        ch.gain_db = gain_db;
    }
}

pub(super) fn apply_mute_update(
    channels: &mut [ChannelInfo],
    group: i32,
    channel: i32,
    muted: bool,
) {
    if let Some(ch) = channels
        .iter_mut()
        .find(|c| c.group == group && c.channel == channel)
    {
        ch.muted = muted;
    }
}

fn mute_arg_to_bool(arg: &crate::lv1::osc::OscArg) -> Option<bool> {
    match arg {
        crate::lv1::osc::OscArg::Bool(value) => Some(*value),
        crate::lv1::osc::OscArg::Int(0) => Some(false),
        crate::lv1::osc::OscArg::Int(1) => Some(true),
        _ => None,
    }
}

pub(super) fn apply_pan_update(
    channels: &mut [ChannelInfo],
    group: i32,
    channel: i32,
    pan: f64,
) -> bool {
    if let Some(ch) = channels
        .iter_mut()
        .find(|c| c.group == group && c.channel == channel)
    {
        ch.pan = Some(pan);
        return true;
    }
    false
}

pub(super) fn apply_balance_update(
    channels: &mut [ChannelInfo],
    group: i32,
    channel: i32,
    balance: f64,
) -> bool {
    if let Some(ch) = channels
        .iter_mut()
        .find(|c| c.group == group && c.channel == channel)
        && matches!(ch.pan_mode, Some(crate::lv1::types::PanMode::Stereo))
    {
        ch.balance = Some(balance);
        return true;
    }
    false
}

pub(super) fn apply_width_update(
    channels: &mut [ChannelInfo],
    group: i32,
    channel: i32,
    width: f64,
) -> bool {
    if let Some(ch) = channels
        .iter_mut()
        .find(|c| c.group == group && c.channel == channel)
    {
        ch.width = Some(width);
        return true;
    }
    false
}

pub(super) struct ActorState {
    generation: u64,
    pub(super) connection: ConnectionStatus,
    pub(super) scene: Option<SceneState>,
    pub(super) scene_list: Vec<SceneListEntry>,
    pub(super) channels: Vec<ChannelInfo>,
    pub(super) ping_sequence: u64,
    pub(super) scene_observation_sequence: u64,
    pub(super) scene_buf: SceneBuffer,
    pub(super) last_ping: Instant,
    pub(super) event_bus: AppEventBus,
}

impl ActorState {
    pub(super) fn new(event_bus: AppEventBus, generation: u64) -> Self {
        Self {
            generation,
            connection: ConnectionStatus::Connecting,
            scene: None,
            scene_list: Vec::new(),
            channels: Vec::new(),
            ping_sequence: 0,
            scene_observation_sequence: 0,
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
            ping_sequence: self.ping_sequence,
        }
    }

    pub(super) fn fan_out(&mut self, event: Lv1Event) {
        self.event_bus.publish_lv1(self.generation, event);
    }

    pub(super) fn diagnose(&mut self, message: impl Into<String>) {
        tracing::debug!(event = "lv1_diagnostic", "{}", message.into());
    }
}

fn observe_scene(state: &mut ActorState, scene: SceneState) {
    state.scene_observation_sequence = state.scene_observation_sequence.saturating_add(1);
    state.scene = Some(scene.clone());
    state.fan_out(Lv1Event::SceneChanged(SceneObservation {
        sequence: state.scene_observation_sequence,
        scene,
    }));
}

pub(super) fn handle_message(state: &mut ActorState, msg: &crate::lv1::osc::OscMessage) {
    if is_diagnostic_address(&msg.address) {
        state.diagnose(format!(
            "received {} args_count={}",
            msg.address,
            msg.args.len()
        ));
    }

    match msg.address.as_str() {
        "/Channels" => match parse_channels_batch(&msg.args) {
            Ok(channels) => {
                state.channels = channels.clone();
                state.fan_out(Lv1Event::ChannelTopologyChanged(channels));
            }
            Err(err) => {
                state.diagnose(format!("failed to parse /Channels: {err}"));
            }
        },
        "/Notify/CurSceneIndex" => {
            if let Some(crate::lv1::osc::OscArg::Int(index)) = msg.args.first()
                && let Some(scene) = state.scene_buf.apply_index(*index)
            {
                observe_scene(state, scene);
            }
        }
        "/Notify/Scene/Name" => {
            if let Some(crate::lv1::osc::OscArg::String(name)) = msg.args.first()
                && let Some(scene) = state.scene_buf.apply_name(name.clone())
            {
                observe_scene(state, scene);
            }
        }
        "/Notify/SceneList" => match parse_scene_list(&msg.args) {
            Ok(list) => {
                state.diagnose(format!("parsed /Notify/SceneList scenes={}", list.len()));
                state.scene_list = list.clone();
                state.fan_out(Lv1Event::SceneListChanged(list));
            }
            Err(err) => {
                state.diagnose(format!("failed to parse /Notify/SceneList: {err}"));
            }
        },
        "/Notify/Track/Out/Gain" => {
            if let (
                Some(crate::lv1::osc::OscArg::Int(group)),
                Some(crate::lv1::osc::OscArg::Int(channel)),
                Some(crate::lv1::osc::OscArg::Double(gain_db)),
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
                Some(crate::lv1::osc::OscArg::Int(group)),
                Some(crate::lv1::osc::OscArg::Int(channel)),
                Some(mute_arg),
            ) = (msg.args.first(), msg.args.get(1), msg.args.get(2))
                && let Some(muted) = mute_arg_to_bool(mute_arg)
            {
                apply_mute_update(&mut state.channels, *group, *channel, muted);
                state.fan_out(Lv1Event::MuteChanged {
                    group: *group,
                    channel: *channel,
                    muted,
                });
            }
        }
        "/Notify/Track/Pan" => {
            if let (
                Some(crate::lv1::osc::OscArg::Int(group)),
                Some(crate::lv1::osc::OscArg::Int(channel)),
                Some(crate::lv1::osc::OscArg::Double(pan)),
            ) = (msg.args.first(), msg.args.get(1), msg.args.get(2))
                && apply_pan_update(&mut state.channels, *group, *channel, *pan)
            {
                state.fan_out(Lv1Event::PanChanged {
                    group: *group,
                    channel: *channel,
                    pan: *pan,
                });
            }
        }
        "/Notify/Balance" => {
            if let (
                Some(crate::lv1::osc::OscArg::Int(group)),
                Some(crate::lv1::osc::OscArg::Int(channel)),
                Some(crate::lv1::osc::OscArg::Double(balance)),
            ) = (msg.args.first(), msg.args.get(1), msg.args.get(2))
                && apply_balance_update(&mut state.channels, *group, *channel, *balance)
            {
                state.fan_out(Lv1Event::BalanceChanged {
                    group: *group,
                    channel: *channel,
                    balance: *balance,
                });
            }
        }
        "/Notify/PanArcWidth" => {
            if let (
                Some(crate::lv1::osc::OscArg::Int(group)),
                Some(crate::lv1::osc::OscArg::Int(channel)),
                Some(crate::lv1::osc::OscArg::Double(width)),
                Some(crate::lv1::osc::OscArg::Int(active)),
            ) = (
                msg.args.first(),
                msg.args.get(1),
                msg.args.get(2),
                msg.args.get(3),
            ) && *active != 0
                && apply_width_update(&mut state.channels, *group, *channel, *width)
            {
                state.fan_out(Lv1Event::WidthChanged {
                    group: *group,
                    channel: *channel,
                    width: *width,
                });
            }
        }
        _ => {}
    }
}

fn is_diagnostic_address(address: &str) -> bool {
    address == "/Channels" || address.split('/').any(|segment| segment.contains("Scene"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(channel: i32) -> ChannelInfo {
        ChannelInfo {
            group: 0,
            channel,
            name: format!("Ch {}", channel + 1),
            gain_db: -9.0,
            muted: false,
            pan: None,
            balance: None,
            width: None,
            pan_mode: None,
        }
    }

    #[test]
    fn scene_buffer_emits_in_either_arrival_order() {
        let mut name_first = SceneBuffer::default();
        assert!(name_first.apply_name("Scene A".to_string()).is_none());
        assert_eq!(
            name_first.apply_index(0).unwrap(),
            SceneState {
                index: 0,
                name: "Scene A".to_string()
            }
        );

        let mut index_first = SceneBuffer::default();
        assert!(index_first.apply_index(1).is_none());
        assert_eq!(
            index_first.apply_name("Scene B".to_string()).unwrap(),
            SceneState {
                index: 1,
                name: "Scene B".to_string()
            }
        );
    }

    #[test]
    fn scene_buffer_overwrites_pending_name() {
        let mut buffer = SceneBuffer::default();
        buffer.apply_name("Old".to_string());
        buffer.apply_name("New".to_string());
        assert_eq!(buffer.apply_index(2).unwrap().name, "New");
    }

    #[test]
    fn apply_fader_update_changes_only_matching_channel() {
        let mut channels = vec![channel(0), channel(1)];
        apply_fader_update(&mut channels, 0, 0, -6.0);
        assert_eq!(channels[0].gain_db, -6.0);
        assert_eq!(channels[1].gain_db, -9.0);
    }

    #[test]
    fn apply_fader_update_ignores_unknown_channel() {
        let mut channels = vec![channel(0)];
        apply_fader_update(&mut channels, 0, 99, -3.0);
        assert_eq!(channels[0].gain_db, -9.0);
    }

    #[test]
    fn apply_mute_update_changes_only_matching_channel() {
        let mut channels = vec![channel(0), channel(1)];
        apply_mute_update(&mut channels, 0, 0, true);
        assert!(channels[0].muted);
        assert!(!channels[1].muted);
    }

    #[test]
    fn apply_mute_update_ignores_unknown_channel() {
        let mut channels = vec![channel(0)];
        apply_mute_update(&mut channels, 0, 99, true);
        assert!(!channels[0].muted);
    }
}

//! LV1 live state mirror helpers.

use std::time::Instant;

use crate::runtime::events::AppEventBus;

use super::events::Lv1Event;
use super::osc::{OscArg, OscMessage};
use super::parsers::{parse_channels_batch, parse_scene_list};
use super::types::{
    ChannelInfo, ConnectionStatus, Lv1StateSnapshot, PanMode, SceneListEntry, SceneObservation,
    SceneState,
};

/// @cc [owner:mixxorz,label:protocol;state] exact-scene-pairing
/// A scene observation MUST contain one index and one name, accept either arrival order, use the
/// latest value when one side repeats before completion, and consume both fields after emission.
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
        let index = self.pending_index?;
        let name = self.pending_name.take()?;
        self.pending_index = None;
        Some(SceneState { index, name })
    }
}

pub(super) fn apply_fader_update(
    channels: &mut [ChannelInfo],
    group: i32,
    channel: i32,
    gain_db: f64,
) -> bool {
    if let Some(ch) = channels
        .iter_mut()
        .find(|c| c.group == group && c.channel == channel)
    {
        ch.gain_db = gain_db;
        return true;
    }
    false
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

pub(super) struct ActorState {
    generation: u64,
    pub(super) connection: ConnectionStatus,
    pub(super) scene: Option<SceneState>,
    pub(super) scene_list: Option<Vec<SceneListEntry>>,
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
            scene_list: None,
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

/// @cc [owner:mixxorz,label:state;generation] connection-local-scene-sequence
/// Each complete scene observation MUST advance the saturating connection-local sequence before
/// updating the mirror and publishing the observation with that same sequence.
fn observe_scene(state: &mut ActorState, scene: SceneState) {
    state.scene_observation_sequence = state.scene_observation_sequence.saturating_add(1);
    state.scene = Some(scene.clone());
    state.fan_out(Lv1Event::SceneChanged(SceneObservation {
        sequence: state.scene_observation_sequence,
        scene,
    }));
}

/**
 * @cc [owner:mixxorz,label:safety;state;parsing] batch-parse-failure-preserves-mirror
 * A channel or scene-list batch that fails parsing MUST preserve the last valid corresponding
 * mirror and MUST NOT publish a topology or scene-list change event.
 */
/**
 * @cc [owner:mixxorz,label:safety;state;protocol] pan-family-applicability
 * Pan-family notifications MUST update and publish only for known channels; balance additionally
 * requires stereo pan mode. Width MUST update and publish only for active flag `i:1`; `i:0` and
 * integers outside the documented binary domain MUST preserve mirrored width and publish nothing.
 */
/**
 * @cc [owner:mixxorz,label:state;protocol] gain-notification-numeric-normalization
 * For a known channel, `/Notify/Track/Out/Gain` MUST accept documented OSC Float, Double, and Int
 * gain values, normalize each to `f64`, and use that same normalized value for the channel mirror
 * and published `FaderChanged` fact. Unknown channels and other value types MUST be ignored without
 * mutating the mirror or publishing `FaderChanged`.
 */
pub(super) fn handle_message(state: &mut ActorState, msg: &OscMessage) {
    if msg.address == "/Channels" || msg.address.contains("Scene") {
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
            if let Some(OscArg::Int(index)) = msg.args.first()
                && let Some(scene) = state.scene_buf.apply_index(*index)
            {
                observe_scene(state, scene);
            }
        }
        "/Notify/Scene/Name" => {
            if let Some(OscArg::String(name)) = msg.args.first()
                && let Some(scene) = state.scene_buf.apply_name(name.clone())
            {
                observe_scene(state, scene);
            }
        }
        "/Notify/SceneList" => match parse_scene_list(&msg.args) {
            Ok(list) => {
                state.diagnose(format!("parsed /Notify/SceneList scenes={}", list.len()));
                state.scene_list = Some(list.clone());
                state.fan_out(Lv1Event::SceneListChanged(list));
            }
            Err(err) => {
                state.diagnose(format!("failed to parse /Notify/SceneList: {err}"));
            }
        },
        "/Notify/Track/Out/Gain" => {
            if let [OscArg::Int(group), OscArg::Int(channel), gain_arg, ..] = msg.args.as_slice() {
                let gain_db = match gain_arg {
                    OscArg::Float(value) => f64::from(*value),
                    OscArg::Double(value) => *value,
                    OscArg::Int(value) => f64::from(*value),
                    _ => return,
                };
                if apply_fader_update(&mut state.channels, *group, *channel, gain_db) {
                    state.fan_out(Lv1Event::FaderChanged {
                        group: *group,
                        channel: *channel,
                        gain_db,
                    });
                }
            }
        }
        "/Notify/Track/Out/Mute" => {
            if let [OscArg::Int(group), OscArg::Int(channel), mute_arg, ..] = msg.args.as_slice() {
                let muted = match mute_arg {
                    OscArg::Bool(value) => *value,
                    OscArg::Int(0) => false,
                    OscArg::Int(1) => true,
                    _ => return,
                };
                apply_mute_update(&mut state.channels, *group, *channel, muted);
                state.fan_out(Lv1Event::MuteChanged {
                    group: *group,
                    channel: *channel,
                    muted,
                });
            }
        }
        "/Notify/Track/Pan" => {
            if let [
                OscArg::Int(group),
                OscArg::Int(channel),
                OscArg::Double(pan),
                ..,
            ] = msg.args.as_slice()
                && let Some(ch) = state
                    .channels
                    .iter_mut()
                    .find(|ch| ch.group == *group && ch.channel == *channel)
            {
                ch.pan = Some(*pan);
                state.fan_out(Lv1Event::PanChanged {
                    group: *group,
                    channel: *channel,
                    pan: *pan,
                });
            }
        }
        "/Notify/Balance" => {
            if let [
                OscArg::Int(group),
                OscArg::Int(channel),
                OscArg::Double(balance),
                ..,
            ] = msg.args.as_slice()
                && let Some(ch) = state
                    .channels
                    .iter_mut()
                    .find(|ch| ch.group == *group && ch.channel == *channel)
                && matches!(ch.pan_mode, Some(PanMode::Stereo))
            {
                ch.balance = Some(*balance);
                state.fan_out(Lv1Event::BalanceChanged {
                    group: *group,
                    channel: *channel,
                    balance: *balance,
                });
            }
        }
        "/Notify/PanArcWidth" => {
            if let [
                OscArg::Int(group),
                OscArg::Int(channel),
                OscArg::Double(width),
                OscArg::Int(1),
                ..,
            ] = msg.args.as_slice()
                && let Some(ch) = state
                    .channels
                    .iter_mut()
                    .find(|ch| ch.group == *group && ch.channel == *channel)
            {
                ch.width = Some(*width);
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
    fn scene_buffer_consumes_both_fields_after_each_observation() {
        let mut buffer = SceneBuffer::default();
        assert!(buffer.apply_index(1).is_none());
        assert!(buffer.apply_index(2).is_none());
        assert_eq!(buffer.apply_name("Latest".to_string()).unwrap().index, 2);
        assert!(buffer.apply_name("Next".to_string()).is_none());
        assert_eq!(buffer.apply_index(3).unwrap().name, "Next");
        assert!(buffer.apply_index(4).is_none());
    }

    #[test]
    fn apply_fader_update_changes_only_matching_channel() {
        let mut channels = vec![channel(0), channel(1)];
        assert!(apply_fader_update(&mut channels, 0, 0, -6.0));
        assert_eq!(channels[0].gain_db, -6.0);
        assert_eq!(channels[1].gain_db, -9.0);
    }

    #[test]
    fn apply_fader_update_ignores_unknown_channel() {
        let mut channels = vec![channel(0)];
        assert!(!apply_fader_update(&mut channels, 0, 99, -3.0));
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

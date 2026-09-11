use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Disconnected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PanMode {
    None,
    Mono,
    Stereo,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneState {
    pub index: i32,
    pub name: String,
}

/// @cc [owner:mixxorz,label:generation;state] scene-observation-sequence-scope
/// `sequence` MUST identify ordering only within one transport connection and MUST NOT be treated as
/// a durable scene identifier or compared across reconnects or generations.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneObservation {
    pub sequence: u64,
    pub scene: SceneState,
}

/// @cc [owner:mixxorz,label:safety;state] recall-dispatch-barrier-semantics
/// `scene_observation_sequence` MUST be the last observation known before recall dispatch; recall
/// completion logic MUST require a matching observation with a strictly greater sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecallSceneDispatch {
    pub scene_observation_sequence: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneListEntry {
    pub index: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChannelInfo {
    pub group: i32,
    pub channel: i32,
    pub name: String,
    pub gain_db: f64,
    pub muted: bool,
    pub pan: Option<f64>,
    pub balance: Option<f64>,
    pub width: Option<f64>,
    pub pan_mode: Option<PanMode>,
}

#[derive(Debug, Clone)]
pub struct Lv1StateSnapshot {
    pub connection: ConnectionStatus,
    pub scene: Option<SceneState>,
    pub scene_list: Vec<SceneListEntry>,
    pub channels: Vec<ChannelInfo>,
    pub ping_sequence: u64,
}

use crate::osc::OscArg;

use super::model::{ChannelInfo, SceneListEntry};

// Each channel record in the /Channels batch has 19 fields:
// [0] s:name, [1] i:group, [2] i:channel, [3] d:gain_db,
// [4..18] other fields (phantom sends, colors, flags — not used in Phase 2)
const CHANNELS_RECORD_STRIDE: usize = 19;

pub fn parse_channels_batch(args: &[OscArg]) -> Result<Vec<ChannelInfo>, &'static str> {
    let count = match args.first() {
        Some(OscArg::Int(n)) => *n as usize,
        _ => return Err("missing or wrong-type count arg"),
    };

    let expected_len = 1 + count * CHANNELS_RECORD_STRIDE;
    if args.len() < expected_len {
        return Err("args too short for declared channel count");
    }

    let mut channels = Vec::with_capacity(count);
    for i in 0..count {
        let base = 1 + i * CHANNELS_RECORD_STRIDE;
        let name = match &args[base] {
            OscArg::String(s) => s.clone(),
            _ => return Err("channel name must be a string"),
        };
        let group = match args[base + 1] {
            OscArg::Int(v) => v,
            _ => return Err("channel group must be an int"),
        };
        let channel = match args[base + 2] {
            OscArg::Int(v) => v,
            _ => return Err("channel index must be an int"),
        };
        let gain_db = match args[base + 3] {
            OscArg::Double(v) => v,
            _ => return Err("channel gain must be a double"),
        };
        channels.push(ChannelInfo {
            group,
            channel,
            name,
            gain_db,
            muted: false,
        });
    }

    Ok(channels)
}

pub fn parse_scene_list(args: &[OscArg]) -> Result<Vec<SceneListEntry>, &'static str> {
    let count = match args.first() {
        Some(OscArg::Int(n)) => *n as usize,
        _ => return Err("missing or wrong-type count arg"),
    };

    let expected_len = 1 + count * 2;
    if args.len() < expected_len {
        return Err("args too short for declared scene count");
    }

    let mut list = Vec::with_capacity(count);
    for i in 0..count {
        let base = 1 + i * 2;
        let index = match args[base] {
            OscArg::Int(v) => v,
            _ => return Err("scene index must be an int"),
        };
        let name = match &args[base + 1] {
            OscArg::String(s) => s.clone(),
            _ => return Err("scene name must be a string"),
        };
        list.push(SceneListEntry { index, name });
    }

    Ok(list)
}

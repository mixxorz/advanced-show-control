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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_channel_args(channels: &[(&str, i32, i32, f64)]) -> Vec<OscArg> {
        let mut args = vec![OscArg::Int(channels.len() as i32)];
        for (name, group, channel, gain_db) in channels {
            args.push(OscArg::String(name.to_string()));
            args.push(OscArg::Int(*group));
            args.push(OscArg::Int(*channel));
            args.push(OscArg::Double(*gain_db));
            for _ in 0..15 {
                args.push(OscArg::Int(0));
            }
        }
        args
    }

    #[test]
    fn parses_channels_batch() {
        let args = make_channel_args(&[("Channel 1", 0, 0, -9.1), ("Fx 1", 2, 0, -12.0)]);
        let channels = parse_channels_batch(&args).unwrap();
        assert_eq!(channels.len(), 2);
        assert_eq!(
            channels[0],
            ChannelInfo {
                group: 0,
                channel: 0,
                name: "Channel 1".to_string(),
                gain_db: -9.1,
                muted: false,
            }
        );
        assert_eq!(
            channels[1],
            ChannelInfo {
                group: 2,
                channel: 0,
                name: "Fx 1".to_string(),
                gain_db: -12.0,
                muted: false,
            }
        );
    }

    #[test]
    fn rejects_channels_batch_with_wrong_arg_count() {
        let args = vec![OscArg::Int(1)];
        assert!(parse_channels_batch(&args).is_err());
    }

    #[test]
    fn rejects_channels_batch_missing_count() {
        assert!(parse_channels_batch(&[]).is_err());
    }

    #[test]
    fn parses_scene_list_with_multiple_scenes() {
        let args = vec![
            OscArg::Int(2),
            OscArg::Int(0),
            OscArg::String("My first scene".to_string()),
            OscArg::Int(1),
            OscArg::String("My second scene".to_string()),
        ];
        let list = parse_scene_list(&args).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(
            list[0],
            SceneListEntry {
                index: 0,
                name: "My first scene".to_string(),
            }
        );
        assert_eq!(
            list[1],
            SceneListEntry {
                index: 1,
                name: "My second scene".to_string(),
            }
        );
    }

    #[test]
    fn parses_empty_scene_list() {
        let args = vec![OscArg::Int(0)];
        let list = parse_scene_list(&args).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn rejects_scene_list_missing_count() {
        assert!(parse_scene_list(&[]).is_err());
    }

    #[test]
    fn channels_default_to_unmuted_when_batch_has_no_mute_field() {
        let args = make_channel_args(&[("Channel 1", 0, 0, -9.1)]);
        let channels = parse_channels_batch(&args).unwrap();
        assert!(!channels[0].muted);
    }
}

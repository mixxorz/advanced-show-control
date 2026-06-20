//! Phase 1 probe logging and message classification.

use crate::lv1::osc::{OscArg, OscMessage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum MessageKind {
    Scene,
    Fader,
    Handshake,
    Keepalive,
    Other,
}

#[derive(Debug, serde::Serialize)]
pub struct ProbeLogEntry {
    pub timestamp_ms: u128,
    pub direction: String,
    pub kind: MessageKind,
    pub address: Option<String>,
    pub args: Vec<String>,
    pub frame_size: Option<usize>,
    pub header_hex: Option<String>,
    pub error: Option<String>,
}

pub fn classify_message(msg: &OscMessage) -> MessageKind {
    let address = msg.address.as_str();
    if is_fader_gain_address(address) {
        MessageKind::Fader
    } else if is_scene_address(address) {
        MessageKind::Scene
    } else if address == "/handshake" || address == "/device_name" {
        MessageKind::Handshake
    } else if address == "/ping" || address == "/pong" {
        MessageKind::Keepalive
    } else {
        MessageKind::Other
    }
}

fn is_fader_gain_address(address: &str) -> bool {
    let segments: Vec<_> = address
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    segments.last() == Some(&"Gain") && segments.contains(&"Track")
}

fn is_scene_address(address: &str) -> bool {
    address
        .split('/')
        .filter(|segment| !segment.is_empty())
        .any(|segment| {
            segment.eq_ignore_ascii_case("Scene") || segment.eq_ignore_ascii_case("CurrentScene")
        })
}

pub fn format_arg(arg: &OscArg) -> String {
    match arg {
        OscArg::Int(value) => format!("i:{value}"),
        OscArg::Float(value) => format!("f:{value}"),
        OscArg::Int64(value) => format!("h:{value}"),
        OscArg::Double(value) => format!("d:{value}"),
        OscArg::String(value) => format!("s:{value}"),
        OscArg::Blob(value) => format!("b:{} bytes", value.len()),
        OscArg::Bool(value) => format!("{}:{}", if *value { 'T' } else { 'F' }, value),
        OscArg::True => "T:true".to_string(),
        OscArg::False => "F:false".to_string(),
        OscArg::Nil => "N:null".to_string(),
        OscArg::Impulse => "I:impulse".to_string(),
    }
}

pub struct JsonlLogger {
    writer: std::io::BufWriter<std::fs::File>,
    start: std::time::Instant,
}

impl JsonlLogger {
    pub fn create(path: &std::path::Path) -> std::io::Result<Self> {
        let file = std::fs::File::create(path)?;
        Ok(Self {
            writer: std::io::BufWriter::new(file),
            start: std::time::Instant::now(),
        })
    }

    pub fn write(&mut self, mut entry: ProbeLogEntry) -> std::io::Result<()> {
        use std::io::Write;
        entry.timestamp_ms = self.start.elapsed().as_millis();
        serde_json::to_writer(&mut self.writer, &entry)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }
}

pub fn entry_for_message(
    direction: &str,
    msg: &OscMessage,
    frame_size: Option<usize>,
    header: Option<[u8; 8]>,
) -> ProbeLogEntry {
    ProbeLogEntry {
        timestamp_ms: 0,
        direction: direction.to_string(),
        kind: classify_message(msg),
        address: Some(msg.address.clone()),
        args: msg.args.iter().map(format_arg).collect(),
        frame_size,
        header_hex: header.map(|bytes| {
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join("")
        }),
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_phase_1_messages() {
        assert_eq!(
            classify_message(&msg("/Notify/Track/Out/Gain")),
            MessageKind::Fader
        );
        assert_eq!(
            classify_message(&msg("/Set/Track/In/Gain")),
            MessageKind::Fader
        );
        assert_eq!(
            classify_message(&msg("/Notify/CurrentScene")),
            MessageKind::Scene
        );
        assert_eq!(classify_message(&msg("/handshake")), MessageKind::Handshake);
        assert_eq!(
            classify_message(&msg("/device_name")),
            MessageKind::Handshake
        );
        assert_eq!(classify_message(&msg("/ping")), MessageKind::Keepalive);
        assert_eq!(classify_message(&msg("/pong")), MessageKind::Keepalive);
        assert_eq!(classify_message(&msg("/Notify/Meters")), MessageKind::Other);
    }

    #[test]
    fn leaves_gain_like_non_fader_messages_as_other() {
        assert_eq!(
            classify_message(&msg("/Notify/Track/Out/GainReduction")),
            MessageKind::Other
        );
        assert_eq!(classify_message(&msg("/Some/GainMode")), MessageKind::Other);
    }

    #[test]
    fn leaves_scene_like_non_scene_messages_as_other() {
        assert_eq!(
            classify_message(&msg("/Notify/Scenery")),
            MessageKind::Other
        );
    }

    #[test]
    fn formats_args_for_logs() {
        assert_eq!(format_arg(&OscArg::Int(3)), "i:3");
        assert_eq!(format_arg(&OscArg::Float(1.5)), "f:1.5");
        assert_eq!(format_arg(&OscArg::Int64(9_000_000_000)), "h:9000000000");
        assert_eq!(format_arg(&OscArg::Double(-12.5)), "d:-12.5");
        assert_eq!(format_arg(&OscArg::String("Lead".to_string())), "s:Lead");
        assert_eq!(format_arg(&OscArg::Blob(vec![1, 2, 3])), "b:3 bytes");
        assert_eq!(format_arg(&OscArg::True), "T:true");
        assert_eq!(format_arg(&OscArg::False), "F:false");
        assert_eq!(format_arg(&OscArg::Nil), "N:null");
        assert_eq!(format_arg(&OscArg::Impulse), "I:impulse");
    }

    #[test]
    fn builds_log_entry_for_message() {
        let msg = OscMessage {
            address: "/Notify/Track/Out/Gain".to_string(),
            args: vec![OscArg::Int(1), OscArg::Double(-6.0)],
        };

        let entry = entry_for_message(
            "received",
            &msg,
            Some(64),
            Some([0x12, 0x34, 0xab, 0xcd, 0, 1, 2, 3]),
        );

        assert_eq!(entry.timestamp_ms, 0);
        assert_eq!(entry.direction, "received");
        assert_eq!(entry.kind, MessageKind::Fader);
        assert_eq!(entry.address.as_deref(), Some("/Notify/Track/Out/Gain"));
        assert_eq!(entry.args, vec!["i:1".to_string(), "d:-6".to_string()]);
        assert_eq!(entry.frame_size, Some(64));
        assert_eq!(entry.header_hex.as_deref(), Some("1234abcd00010203"));
        assert_eq!(entry.error, None);
    }

    #[test]
    fn writes_log_entries_as_json_lines() {
        let path = std::env::temp_dir().join(format!(
            "lv1-probe-test-{}-{}.jsonl",
            std::process::id(),
            unique_suffix()
        ));
        let mut logger = JsonlLogger::create(&path).unwrap();

        logger
            .write(ProbeLogEntry {
                timestamp_ms: 0,
                direction: "received".to_string(),
                kind: MessageKind::Keepalive,
                address: Some("/ping".to_string()),
                args: vec![],
                frame_size: Some(8),
                header_hex: Some("0000000000000008".to_string()),
                error: None,
            })
            .unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let line = contents.lines().next().unwrap();
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(contents.lines().count(), 1);
        assert!(value["timestamp_ms"].as_u64().is_some());
        assert_eq!(value["direction"], "received");
        assert_eq!(value["kind"], "Keepalive");
        assert_eq!(value["address"], "/ping");
    }

    fn msg(address: &str) -> OscMessage {
        OscMessage {
            address: address.to_string(),
            args: vec![],
        }
    }

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }
}

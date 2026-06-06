# Phase 1 Protocol Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust Phase 1 LV1 protocol probe with reusable OSC, discovery, TCP framing/client, logging, and CLI commands.

**Architecture:** The crate becomes a library plus CLI binary. `src/osc.rs` owns pure OSC encoding/decoding, `src/lv1/discovery.rs` owns Waves zDNS multicast parsing/listening, `src/lv1/tcp.rs` owns LV1 OSC-over-TCP framing and connection behavior, and `src/lv1/probe.rs` owns Phase 1 logging/classification. `src/main.rs` stays thin and wires CLI commands to library modules.

**Tech Stack:** Rust 2024, `clap` for CLI parsing, `serde`/`serde_json` for structured output/logs, `thiserror` for typed errors, `uuid` for device IDs, `socket2` for multicast-friendly UDP setup when needed, standard `std::net` TCP/UDP for networking.

---

## File Structure

- Modify: `Cargo.toml` to add dependencies and expose a library target.
- Create: `src/lib.rs` to export `osc` and `lv1` modules.
- Create: `src/osc.rs` for OSC types, encoder, decoder, and tests.
- Create: `src/lv1/mod.rs` for LV1 module exports.
- Create: `src/lv1/discovery.rs` for `/zDNS` parsing, IP ranking, discovery socket loop, and tests.
- Create: `src/lv1/tcp.rs` for frame encode/decode, TCP client, handshake, keepalive, and tests.
- Create: `src/lv1/probe.rs` for log entries, message classification, JSONL writer, and tests.
- Modify: `src/main.rs` to implement `discover`, `listen`, and `set-gain` CLI commands.

---

### Task 1: Crate Setup

**Files:**
- Modify: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/lv1/mod.rs`

- [ ] **Step 1: Add dependencies and library entry point**

Replace `Cargo.toml` with:

```toml
[package]
name = "lv1-scene-fade-utility"
version = "0.1.0"
edition = "2024"

[lib]
name = "lv1_scene_fade_utility"
path = "src/lib.rs"

[dependencies]
clap = { version = "4.5", features = ["derive"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
uuid = { version = "1.8", features = ["v4"] }
```

- [ ] **Step 2: Create module roots**

Create `src/lib.rs`:

```rust
pub mod lv1;
pub mod osc;
```

Create `src/lv1/mod.rs`:

```rust
pub mod discovery;
pub mod probe;
pub mod tcp;
```

- [ ] **Step 3: Run check to verify setup fails only because modules are missing**

Run: `cargo check`

Expected: FAIL with missing file errors for `src/osc.rs`, `src/lv1/discovery.rs`, `src/lv1/probe.rs`, and `src/lv1/tcp.rs`.

- [ ] **Step 4: Create empty module files**

Create `src/osc.rs`:

```rust
//! Minimal OSC 1.0 encoding and decoding used by the LV1 protocol probe.
```

Create `src/lv1/discovery.rs`:

```rust
//! Waves LV1 custom /zDNS discovery.
```

Create `src/lv1/tcp.rs`:

```rust
//! Waves LV1 OSC-over-TCP framing and client behavior.
```

Create `src/lv1/probe.rs`:

```rust
//! Phase 1 probe logging and message classification.
```

- [ ] **Step 5: Verify crate compiles**

Run: `cargo check`

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/lv1/mod.rs src/osc.rs src/lv1/discovery.rs src/lv1/tcp.rs src/lv1/probe.rs
git commit -m "chore: set up protocol probe crate"
```

---

### Task 2: OSC Encoder And Decoder

**Files:**
- Modify: `src/osc.rs`

- [ ] **Step 1: Write failing OSC tests**

Replace `src/osc.rs` with this test-first scaffold:

```rust
//! Minimal OSC 1.0 encoding and decoding used by the LV1 protocol probe.

#[derive(Debug, Clone, PartialEq)]
pub enum OscArg {
    Int(i32),
    Float(f32),
    Int64(i64),
    Double(f64),
    String(String),
    Blob(Vec<u8>),
    True,
    False,
    Nil,
    Impulse,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OscMessage {
    pub address: String,
    pub args: Vec<OscArg>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum OscError {
    #[error("OSC string is not null terminated")]
    UnterminatedString,
    #[error("OSC packet ended while reading {0}")]
    UnexpectedEof(&'static str),
    #[error("OSC type tag string must start with comma")]
    InvalidTypeTag,
    #[error("unsupported OSC type tag: {0}")]
    UnsupportedType(char),
}

pub fn encode_message(_address: &str, _args: &[OscArg]) -> Result<Vec<u8>, OscError> {
    unimplemented!("implemented in this task")
}

pub fn decode_packet(_bytes: &[u8]) -> Result<OscMessage, OscError> {
    unimplemented!("implemented in this task")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes_numeric_and_string_args() {
        let msg = OscMessage {
            address: "/Set/Track/Out/Gain".to_string(),
            args: vec![OscArg::Int(0), OscArg::Int(1), OscArg::Double(-12.5)],
        };

        let bytes = encode_message(&msg.address, &msg.args).unwrap();
        assert_eq!(decode_packet(&bytes).unwrap(), msg);
    }

    #[test]
    fn encodes_and_decodes_all_phase_1_types() {
        let msg = OscMessage {
            address: "/types".to_string(),
            args: vec![
                OscArg::Int(-1),
                OscArg::Float(1.5),
                OscArg::Int64(9_000_000_000),
                OscArg::Double(-3.25),
                OscArg::String("lv1".to_string()),
                OscArg::Blob(vec![1, 2, 3]),
                OscArg::True,
                OscArg::False,
                OscArg::Nil,
                OscArg::Impulse,
            ],
        };

        let bytes = encode_message(&msg.address, &msg.args).unwrap();
        assert_eq!(decode_packet(&bytes).unwrap(), msg);
    }

    #[test]
    fn pads_strings_and_blobs_to_four_byte_boundaries() {
        let bytes = encode_message(
            "/pad",
            &[OscArg::String("abc".to_string()), OscArg::Blob(vec![1, 2, 3])],
        )
        .unwrap();

        assert_eq!(bytes.len() % 4, 0);
        assert_eq!(
            decode_packet(&bytes).unwrap(),
            OscMessage {
                address: "/pad".to_string(),
                args: vec![OscArg::String("abc".to_string()), OscArg::Blob(vec![1, 2, 3])],
            }
        );
    }

    #[test]
    fn rejects_unterminated_strings() {
        assert_eq!(decode_packet(b"/bad"), Err(OscError::UnterminatedString));
    }
}
```

- [ ] **Step 2: Run OSC tests to verify failure**

Run: `cargo test osc::tests -- --nocapture`

Expected: FAIL because `encode_message` and `decode_packet` are not implemented.

- [ ] **Step 3: Implement OSC encode/decode**

Replace the two unimplemented functions and add helpers above the tests:

```rust
fn pad_to_4(len: usize) -> usize {
    (4 - (len % 4)) % 4
}

fn encode_string(value: &str, out: &mut Vec<u8>) {
    out.extend_from_slice(value.as_bytes());
    out.push(0);
    out.extend(std::iter::repeat_n(0, pad_to_4(value.len() + 1)));
}

fn read_string(bytes: &[u8], offset: &mut usize) -> Result<String, OscError> {
    let start = *offset;
    let mut end = start;
    while end < bytes.len() && bytes[end] != 0 {
        end += 1;
    }
    if end >= bytes.len() {
        return Err(OscError::UnterminatedString);
    }
    let value = String::from_utf8_lossy(&bytes[start..end]).to_string();
    let raw_len = end - start + 1;
    *offset += raw_len + pad_to_4(raw_len);
    Ok(value)
}

fn take<const N: usize>(bytes: &[u8], offset: &mut usize, label: &'static str) -> Result<[u8; N], OscError> {
    if *offset + N > bytes.len() {
        return Err(OscError::UnexpectedEof(label));
    }
    let mut out = [0_u8; N];
    out.copy_from_slice(&bytes[*offset..*offset + N]);
    *offset += N;
    Ok(out)
}

pub fn encode_message(address: &str, args: &[OscArg]) -> Result<Vec<u8>, OscError> {
    let mut out = Vec::new();
    encode_string(address, &mut out);

    let mut tags = String::from(",");
    for arg in args {
        tags.push(match arg {
            OscArg::Int(_) => 'i',
            OscArg::Float(_) => 'f',
            OscArg::Int64(_) => 'h',
            OscArg::Double(_) => 'd',
            OscArg::String(_) => 's',
            OscArg::Blob(_) => 'b',
            OscArg::True => 'T',
            OscArg::False => 'F',
            OscArg::Nil => 'N',
            OscArg::Impulse => 'I',
        });
    }
    encode_string(&tags, &mut out);

    for arg in args {
        match arg {
            OscArg::Int(value) => out.extend_from_slice(&value.to_be_bytes()),
            OscArg::Float(value) => out.extend_from_slice(&value.to_be_bytes()),
            OscArg::Int64(value) => out.extend_from_slice(&value.to_be_bytes()),
            OscArg::Double(value) => out.extend_from_slice(&value.to_be_bytes()),
            OscArg::String(value) => encode_string(value, &mut out),
            OscArg::Blob(value) => {
                out.extend_from_slice(&(value.len() as i32).to_be_bytes());
                out.extend_from_slice(value);
                out.extend(std::iter::repeat_n(0, pad_to_4(value.len())));
            }
            OscArg::True | OscArg::False | OscArg::Nil | OscArg::Impulse => {}
        }
    }

    Ok(out)
}

pub fn decode_packet(bytes: &[u8]) -> Result<OscMessage, OscError> {
    let mut offset = 0;
    let address = read_string(bytes, &mut offset)?;
    if offset >= bytes.len() {
        return Ok(OscMessage { address, args: vec![] });
    }

    let tags = read_string(bytes, &mut offset)?;
    if !tags.starts_with(',') {
        return Err(OscError::InvalidTypeTag);
    }

    let mut args = Vec::new();
    for tag in tags[1..].chars() {
        let arg = match tag {
            'i' => OscArg::Int(i32::from_be_bytes(take(bytes, &mut offset, "int32")?)),
            'f' => OscArg::Float(f32::from_be_bytes(take(bytes, &mut offset, "float32")?)),
            'h' => OscArg::Int64(i64::from_be_bytes(take(bytes, &mut offset, "int64")?)),
            'd' => OscArg::Double(f64::from_be_bytes(take(bytes, &mut offset, "float64")?)),
            's' => OscArg::String(read_string(bytes, &mut offset)?),
            'b' => {
                let len = i32::from_be_bytes(take(bytes, &mut offset, "blob length")?) as usize;
                if offset + len > bytes.len() {
                    return Err(OscError::UnexpectedEof("blob"));
                }
                let value = bytes[offset..offset + len].to_vec();
                offset += len + pad_to_4(len);
                OscArg::Blob(value)
            }
            'T' => OscArg::True,
            'F' => OscArg::False,
            'N' => OscArg::Nil,
            'I' => OscArg::Impulse,
            other => return Err(OscError::UnsupportedType(other)),
        };
        args.push(arg);
    }

    Ok(OscMessage { address, args })
}
```

- [ ] **Step 4: Run OSC tests**

Run: `cargo test osc::tests -- --nocapture`

Expected: PASS, all 4 tests pass.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/osc.rs
git commit -m "feat: add minimal osc codec"
```

---

### Task 3: LV1 TCP Framing

**Files:**
- Modify: `src/lv1/tcp.rs`

- [ ] **Step 1: Write failing framing tests**

Replace `src/lv1/tcp.rs` with:

```rust
//! Waves LV1 OSC-over-TCP framing and client behavior.

use crate::osc::{OscArg, OscError, OscMessage, decode_packet, encode_message};

pub const DEFAULT_HEADER: [u8; 8] = [0, 0, 0, 2, 0, 0, 0, 0];
const HEADER_LEN: usize = 8;
const MAX_FRAME_PAYLOAD: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Lv1Frame {
    pub header: [u8; 8],
    pub payload: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum Lv1TcpError {
    #[error("OSC error: {0}")]
    Osc(#[from] OscError),
    #[error("invalid LV1 payload length: {0}")]
    InvalidLength(usize),
}

pub fn encode_frame(_address: &str, _args: &[OscArg]) -> Result<Vec<u8>, Lv1TcpError> {
    unimplemented!("implemented in this task")
}

#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub fn push(&mut self, _bytes: &[u8]) -> Result<Vec<Lv1Frame>, Lv1TcpError> {
        unimplemented!("implemented in this task")
    }
}

pub fn decode_frame_payload(frame: &Lv1Frame) -> Result<OscMessage, Lv1TcpError> {
    Ok(decode_packet(&frame.payload)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_frame_with_payload_length_and_default_header() {
        let frame = encode_frame("/ping", &[OscArg::Int64(123), OscArg::Int(7)]).unwrap();

        let payload_len = u32::from_be_bytes(frame[0..4].try_into().unwrap()) as usize;
        assert_eq!(&frame[4..12], &DEFAULT_HEADER);
        assert_eq!(payload_len, frame.len() - 12);
    }

    #[test]
    fn decodes_partial_tcp_reads_into_complete_frames() {
        let bytes = encode_frame("/handshake", &[OscArg::Int(1)]).unwrap();
        let split = bytes.len() / 2;
        let mut decoder = FrameDecoder::default();

        assert!(decoder.push(&bytes[..split]).unwrap().is_empty());
        let frames = decoder.push(&bytes[split..]).unwrap();

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].header, DEFAULT_HEADER);
        assert_eq!(decode_frame_payload(&frames[0]).unwrap().address, "/handshake");
    }

    #[test]
    fn rejects_impossible_lengths() {
        let mut decoder = FrameDecoder::default();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&((MAX_FRAME_PAYLOAD as u32) + 1).to_be_bytes());
        bytes.extend_from_slice(&DEFAULT_HEADER);

        assert!(matches!(decoder.push(&bytes), Err(Lv1TcpError::InvalidLength(_))));
    }
}
```

- [ ] **Step 2: Run framing tests to verify failure**

Run: `cargo test lv1::tcp::tests -- --nocapture`

Expected: FAIL because `encode_frame` and `FrameDecoder::push` are not implemented.

- [ ] **Step 3: Implement framing**

Replace `encode_frame` and `FrameDecoder::push` with:

```rust
pub fn encode_frame(address: &str, args: &[OscArg]) -> Result<Vec<u8>, Lv1TcpError> {
    let payload = encode_message(address, args)?;
    let mut frame = Vec::with_capacity(4 + HEADER_LEN + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&DEFAULT_HEADER);
    frame.extend_from_slice(&payload);
    Ok(frame)
}

impl FrameDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Lv1Frame>, Lv1TcpError> {
        self.buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();

        loop {
            if self.buffer.len() < 4 + HEADER_LEN {
                break;
            }

            let payload_len = u32::from_be_bytes(self.buffer[0..4].try_into().unwrap()) as usize;
            if payload_len == 0 || payload_len > MAX_FRAME_PAYLOAD {
                return Err(Lv1TcpError::InvalidLength(payload_len));
            }

            let total_len = 4 + HEADER_LEN + payload_len;
            if self.buffer.len() < total_len {
                break;
            }

            let mut header = [0_u8; HEADER_LEN];
            header.copy_from_slice(&self.buffer[4..12]);
            let payload = self.buffer[12..total_len].to_vec();
            self.buffer.drain(..total_len);
            frames.push(Lv1Frame { header, payload });
        }

        Ok(frames)
    }
}
```

- [ ] **Step 4: Run framing tests**

Run: `cargo test lv1::tcp::tests -- --nocapture`

Expected: PASS, all 3 tests pass.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/lv1/tcp.rs
git commit -m "feat: add lv1 tcp framing"
```

---

### Task 4: zDNS Discovery Parser

**Files:**
- Modify: `src/lv1/discovery.rs`

- [ ] **Step 1: Write failing parser and ranking tests**

Replace `src/lv1/discovery.rs` with:

```rust
//! Waves LV1 custom /zDNS discovery.

use crate::osc::{OscArg, OscError, decode_packet};

pub const MCAST_ADDR: &str = "225.1.1.1";
pub const MCAST_PORT: u16 = 13337;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DiscoveryEntry {
    pub service: String,
    pub uuid: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub addresses: Vec<String>,
    pub ipv6: Vec<String>,
    pub source: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("OSC error: {0}")]
    Osc(#[from] OscError),
    #[error("not a zDNS packet")]
    NotZdns,
}

pub fn rank_ip(_ip: &str) -> i32 {
    unimplemented!("implemented in this task")
}

pub fn parse_zdns_packet(_bytes: &[u8], _source: &str) -> Result<DiscoveryEntry, DiscoveryError> {
    unimplemented!("implemented in this task")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osc::encode_message;

    #[test]
    fn parses_zdns_packet_and_ranks_ipv4_addresses() {
        let packet = encode_message(
            "/zDNS",
            &[
                OscArg::String("_waveslv113._tcp".to_string()),
                OscArg::String("uuid-1".to_string()),
                OscArg::String("lv1-host".to_string()),
                OscArg::Int(50000),
                OscArg::String("172.20.1.9".to_string()),
                OscArg::String("192.168.1.10".to_string()),
                OscArg::String("fe80::1".to_string()),
            ],
        )
        .unwrap();

        let entry = parse_zdns_packet(&packet, "192.168.1.10").unwrap();

        assert_eq!(entry.service, "_waveslv113._tcp");
        assert_eq!(entry.uuid.as_deref(), Some("uuid-1"));
        assert_eq!(entry.host.as_deref(), Some("lv1-host"));
        assert_eq!(entry.port, Some(50000));
        assert_eq!(entry.addresses, vec!["192.168.1.10", "172.20.1.9"]);
        assert_eq!(entry.ipv6, vec!["fe80::1"]);
        assert_eq!(entry.source, "192.168.1.10");
    }

    #[test]
    fn rejects_non_zdns_packets() {
        let packet = encode_message("/not-zdns", &[]).unwrap();
        assert!(matches!(parse_zdns_packet(&packet, "127.0.0.1"), Err(DiscoveryError::NotZdns)));
    }

    #[test]
    fn ranks_likely_lan_addresses_highest() {
        assert!(rank_ip("192.168.1.10") > rank_ip("172.20.1.9"));
        assert!(rank_ip("10.0.0.4") > rank_ip("169.254.1.1"));
        assert!(rank_ip("127.0.0.1") < rank_ip("169.254.1.1"));
    }
}
```

- [ ] **Step 2: Run discovery tests to verify failure**

Run: `cargo test lv1::discovery::tests -- --nocapture`

Expected: FAIL because parser and ranking are unimplemented.

- [ ] **Step 3: Implement parser and ranking**

Add helper functions and replace `rank_ip` and `parse_zdns_packet`:

```rust
fn ipv4_like(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 4 && parts.iter().all(|part| part.parse::<u8>().is_ok())
}

fn ipv6_like(value: &str) -> bool {
    value.contains(':')
}

pub fn rank_ip(ip: &str) -> i32 {
    if ip.starts_with("127.") {
        -100
    } else if ip.starts_with("169.254.") {
        -50
    } else if ip.starts_with("192.168.56.") {
        20
    } else if ip.starts_with("172.") {
        let second = ip.split('.').nth(1).and_then(|value| value.parse::<u8>().ok()).unwrap_or(0);
        if (16..=31).contains(&second) { 30 } else { 40 }
    } else if ip.starts_with("192.168.") {
        100
    } else if ip.starts_with("10.") {
        90
    } else {
        40
    }
}

pub fn parse_zdns_packet(bytes: &[u8], source: &str) -> Result<DiscoveryEntry, DiscoveryError> {
    let msg = decode_packet(bytes)?;
    if msg.address != "/zDNS" {
        return Err(DiscoveryError::NotZdns);
    }

    let Some(OscArg::String(service)) = msg.args.first() else {
        return Err(DiscoveryError::NotZdns);
    };

    let uuid = match msg.args.get(1) {
        Some(OscArg::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => None,
    };

    let mut host = None;
    let mut port = None;
    let mut addresses = Vec::new();
    let mut ipv6 = Vec::new();

    for arg in msg.args.iter().skip(2) {
        match arg {
            OscArg::String(value) if ipv4_like(value) => addresses.push(value.clone()),
            OscArg::String(value) if ipv6_like(value) => ipv6.push(value.clone()),
            OscArg::String(value) if host.is_none() && !value.is_empty() => host = Some(value.clone()),
            OscArg::Int(value) if port.is_none() && *value > 1024 && *value < 65536 => port = Some(*value as u16),
            _ => {}
        }
    }

    addresses.sort_by_key(|ip| std::cmp::Reverse(rank_ip(ip)));

    Ok(DiscoveryEntry {
        service: service.clone(),
        uuid,
        host,
        port,
        addresses,
        ipv6,
        source: source.to_string(),
    })
}
```

- [ ] **Step 4: Run discovery parser tests**

Run: `cargo test lv1::discovery::tests -- --nocapture`

Expected: PASS, all 3 tests pass.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/lv1/discovery.rs
git commit -m "feat: parse lv1 zdns discovery packets"
```

---

### Task 5: Discovery Socket Loop

**Files:**
- Modify: `src/lv1/discovery.rs`

- [ ] **Step 1: Add discovery API test for filtering behavior**

Add this test inside `src/lv1/discovery.rs` tests module:

```rust
#[test]
fn filter_entry_by_service_and_host_ip() {
    let entry = DiscoveryEntry {
        service: "_waveslv113._tcp".to_string(),
        uuid: Some("uuid-1".to_string()),
        host: Some("lv1-host".to_string()),
        port: Some(50000),
        addresses: vec!["192.168.1.10".to_string()],
        ipv6: vec![],
        source: "192.168.1.10".to_string(),
    };

    assert!(entry_matches(&entry, "_waveslv113._tcp", None));
    assert!(entry_matches(&entry, "_waveslv113._tcp", Some("192.168.1.10")));
    assert!(!entry_matches(&entry, "_waveslv113._tcp", Some("10.0.0.4")));
    assert!(!entry_matches(&entry, "_other._tcp", None));
}
```

Add these declarations above tests:

```rust
#[derive(Debug, Clone)]
pub struct DiscoverOptions {
    pub timeout: std::time::Duration,
    pub filter_host_ip: Option<String>,
    pub filter_service: String,
}

impl Default for DiscoverOptions {
    fn default() -> Self {
        Self {
            timeout: std::time::Duration::from_millis(6000),
            filter_host_ip: None,
            filter_service: "_waveslv113._tcp".to_string(),
        }
    }
}

pub fn entry_matches(_entry: &DiscoveryEntry, _service: &str, _host_ip: Option<&str>) -> bool {
    unimplemented!("implemented in this task")
}
```

- [ ] **Step 2: Run targeted test to verify failure**

Run: `cargo test lv1::discovery::tests::filter_entry_by_service_and_host_ip -- --nocapture`

Expected: FAIL because `entry_matches` is unimplemented.

- [ ] **Step 3: Implement filtering and discovery loop**

Replace `entry_matches` and add `discover` below it:

```rust
pub fn entry_matches(entry: &DiscoveryEntry, service: &str, host_ip: Option<&str>) -> bool {
    if entry.service != service {
        return false;
    }
    match host_ip {
        Some(ip) => entry.addresses.iter().any(|address| address == ip),
        None => true,
    }
}

pub fn discover(options: DiscoverOptions) -> std::io::Result<Vec<DiscoveryEntry>> {
    use std::collections::BTreeMap;
    use std::net::{Ipv4Addr, UdpSocket};
    use std::time::Instant;

    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, MCAST_PORT))?;
    socket.set_read_timeout(Some(std::time::Duration::from_millis(250)))?;
    socket.set_multicast_loop_v4(true)?;
    socket.join_multicast_v4(&MCAST_ADDR.parse::<Ipv4Addr>().unwrap(), &Ipv4Addr::UNSPECIFIED)?;

    let deadline = Instant::now() + options.timeout;
    let mut found = BTreeMap::<String, DiscoveryEntry>::new();
    let mut buf = [0_u8; 65_536];

    while Instant::now() < deadline {
        match socket.recv_from(&mut buf) {
            Ok((size, source)) => {
                if let Ok(entry) = parse_zdns_packet(&buf[..size], &source.ip().to_string()) {
                    if entry_matches(&entry, &options.filter_service, options.filter_host_ip.as_deref()) {
                        let key = format!("{}|{:?}|{:?}", entry.service, entry.host, entry.port);
                        found.entry(key).or_insert(entry);
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock || err.kind() == std::io::ErrorKind::TimedOut => {}
            Err(err) => return Err(err),
        }
    }

    Ok(found.into_values().collect())
}
```

- [ ] **Step 4: Run discovery tests**

Run: `cargo test lv1::discovery::tests -- --nocapture`

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/lv1/discovery.rs
git commit -m "feat: add lv1 discovery scan"
```

---

### Task 6: TCP Client Handshake And Keepalive

**Files:**
- Modify: `src/lv1/tcp.rs`

- [ ] **Step 1: Add tests for handshake batch and ping detection**

Add these tests inside `src/lv1/tcp.rs` tests module:

```rust
#[test]
fn builds_myfoh_handshake_batch() {
    let bytes = build_myfoh_handshake_batch("lv1-probe", "uuid-1").unwrap();
    let mut decoder = FrameDecoder::default();
    let frames = decoder.push(&bytes).unwrap();

    assert_eq!(frames.len(), 2);
    assert_eq!(decode_frame_payload(&frames[0]).unwrap().address, "/handshake");
    assert_eq!(decode_frame_payload(&frames[1]).unwrap().address, "/device_name");
}

#[test]
fn identifies_ping_and_builds_matching_pong() {
    let ping = OscMessage {
        address: "/ping".to_string(),
        args: vec![OscArg::Int64(123), OscArg::Int(7)],
    };

    let pong = pong_for_ping(&ping).unwrap();

    assert_eq!(pong.0, "/pong");
    assert_eq!(pong.1, ping.args);
}
```

Add these declarations above tests:

```rust
pub fn build_myfoh_handshake_batch(_device_name: &str, _uuid: &str) -> Result<Vec<u8>, Lv1TcpError> {
    unimplemented!("implemented in this task")
}

pub fn pong_for_ping(_msg: &OscMessage) -> Option<(&'static str, Vec<OscArg>)> {
    unimplemented!("implemented in this task")
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test lv1::tcp::tests -- --nocapture`

Expected: FAIL for the new unimplemented functions.

- [ ] **Step 3: Implement handshake batch and ping helper**

Replace the declarations with:

```rust
pub fn build_myfoh_handshake_batch(device_name: &str, uuid: &str) -> Result<Vec<u8>, Lv1TcpError> {
    let mut out = Vec::new();
    out.extend_from_slice(&encode_frame(
        "/handshake",
        &[OscArg::Int(1), OscArg::Int(-1), OscArg::Int(1)],
    )?);
    out.extend_from_slice(&encode_frame(
        "/device_name",
        &[OscArg::String(device_name.to_string()), OscArg::String(uuid.to_string())],
    )?);
    Ok(out)
}

pub fn pong_for_ping(msg: &OscMessage) -> Option<(&'static str, Vec<OscArg>)> {
    if msg.address == "/ping" {
        Some(("/pong", msg.args.clone()))
    } else {
        None
    }
}
```

- [ ] **Step 4: Add blocking client type**

Add this below helper functions:

```rust
pub struct Lv1TcpClient {
    stream: std::net::TcpStream,
    decoder: FrameDecoder,
}

impl Lv1TcpClient {
    pub fn connect(host: &str, port: u16) -> std::io::Result<Self> {
        let stream = std::net::TcpStream::connect((host, port))?;
        stream.set_read_timeout(Some(std::time::Duration::from_millis(250)))?;
        Ok(Self { stream, decoder: FrameDecoder::default() })
    }

    pub fn register_myfoh(&mut self, device_name: &str, uuid: &str) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;
        let batch = build_myfoh_handshake_batch(device_name, uuid)?;
        self.stream.write_all(&batch)?;
        Ok(())
    }

    pub fn send(&mut self, address: &str, args: &[OscArg]) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;
        let frame = encode_frame(address, args)?;
        self.stream.write_all(&frame)?;
        Ok(())
    }

    pub fn read_available(&mut self) -> Result<Vec<Lv1Frame>, Box<dyn std::error::Error>> {
        use std::io::Read;
        let mut buf = [0_u8; 8192];
        match self.stream.read(&mut buf) {
            Ok(0) => Ok(Vec::new()),
            Ok(size) => Ok(self.decoder.push(&buf[..size])?),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock || err.kind() == std::io::ErrorKind::TimedOut => Ok(Vec::new()),
            Err(err) => Err(Box::new(err)),
        }
    }
}
```

- [ ] **Step 5: Run TCP tests**

Run: `cargo test lv1::tcp::tests -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add src/lv1/tcp.rs
git commit -m "feat: add lv1 handshake helpers"
```

---

### Task 7: Probe Logging And Classification

**Files:**
- Modify: `src/lv1/probe.rs`

- [ ] **Step 1: Write failing classification tests**

Replace `src/lv1/probe.rs` with:

```rust
//! Phase 1 probe logging and message classification.

use crate::osc::{OscArg, OscMessage};

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

pub fn classify_message(_msg: &OscMessage) -> MessageKind {
    unimplemented!("implemented in this task")
}

pub fn format_arg(_arg: &OscArg) -> String {
    unimplemented!("implemented in this task")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_phase_1_messages() {
        assert_eq!(classify_message(&msg("/Notify/Track/Out/Gain")), MessageKind::Fader);
        assert_eq!(classify_message(&msg("/Notify/CurrentScene")), MessageKind::Scene);
        assert_eq!(classify_message(&msg("/handshake")), MessageKind::Handshake);
        assert_eq!(classify_message(&msg("/ping")), MessageKind::Keepalive);
        assert_eq!(classify_message(&msg("/pong")), MessageKind::Keepalive);
        assert_eq!(classify_message(&msg("/Notify/Meters")), MessageKind::Other);
    }

    #[test]
    fn formats_args_for_logs() {
        assert_eq!(format_arg(&OscArg::Int(3)), "i:3");
        assert_eq!(format_arg(&OscArg::Double(-12.5)), "d:-12.5");
        assert_eq!(format_arg(&OscArg::String("Lead".to_string())), "s:Lead");
        assert_eq!(format_arg(&OscArg::True), "T:true");
    }

    fn msg(address: &str) -> OscMessage {
        OscMessage { address: address.to_string(), args: vec![] }
    }
}
```

- [ ] **Step 2: Run probe tests to verify failure**

Run: `cargo test lv1::probe::tests -- --nocapture`

Expected: FAIL because classifier and formatter are unimplemented.

- [ ] **Step 3: Implement classifier and formatter**

Replace `classify_message` and `format_arg` with:

```rust
pub fn classify_message(msg: &OscMessage) -> MessageKind {
    let address = msg.address.as_str();
    if address == "/Notify/Track/Out/Gain" || address.contains("/Gain") {
        MessageKind::Fader
    } else if address.to_ascii_lowercase().contains("scene") {
        MessageKind::Scene
    } else if address == "/handshake" || address == "/device_name" {
        MessageKind::Handshake
    } else if address == "/ping" || address == "/pong" {
        MessageKind::Keepalive
    } else {
        MessageKind::Other
    }
}

pub fn format_arg(arg: &OscArg) -> String {
    match arg {
        OscArg::Int(value) => format!("i:{value}"),
        OscArg::Float(value) => format!("f:{value}"),
        OscArg::Int64(value) => format!("h:{value}"),
        OscArg::Double(value) => format!("d:{value}"),
        OscArg::String(value) => format!("s:{value}"),
        OscArg::Blob(value) => format!("b:{} bytes", value.len()),
        OscArg::True => "T:true".to_string(),
        OscArg::False => "F:false".to_string(),
        OscArg::Nil => "N:null".to_string(),
        OscArg::Impulse => "I:impulse".to_string(),
    }
}
```

- [ ] **Step 4: Add JSONL writer helper**

Add below `format_arg`:

```rust
pub struct JsonlLogger {
    writer: std::io::BufWriter<std::fs::File>,
    start: std::time::Instant,
}

impl JsonlLogger {
    pub fn create(path: &std::path::Path) -> std::io::Result<Self> {
        let file = std::fs::File::create(path)?;
        Ok(Self { writer: std::io::BufWriter::new(file), start: std::time::Instant::now() })
    }

    pub fn write(&mut self, mut entry: ProbeLogEntry) -> std::io::Result<()> {
        use std::io::Write;
        entry.timestamp_ms = self.start.elapsed().as_millis();
        serde_json::to_writer(&mut self.writer, &entry)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }
}

pub fn entry_for_message(direction: &str, msg: &OscMessage, frame_size: Option<usize>, header: Option<[u8; 8]>) -> ProbeLogEntry {
    ProbeLogEntry {
        timestamp_ms: 0,
        direction: direction.to_string(),
        kind: classify_message(msg),
        address: Some(msg.address.clone()),
        args: msg.args.iter().map(format_arg).collect(),
        frame_size,
        header_hex: header.map(|bytes| bytes.iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join("")),
        error: None,
    }
}
```

- [ ] **Step 5: Run probe tests**

Run: `cargo test lv1::probe::tests -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add src/lv1/probe.rs
git commit -m "feat: add protocol probe logging"
```

---

### Task 8: CLI Commands

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Replace hello world with CLI structure**

Replace `src/main.rs` with:

```rust
use clap::{Parser, Subcommand};
use lv1_scene_fade_utility::lv1::discovery::{DiscoverOptions, discover};
use lv1_scene_fade_utility::lv1::probe::{JsonlLogger, MessageKind, entry_for_message};
use lv1_scene_fade_utility::lv1::tcp::{Lv1TcpClient, decode_frame_payload, pong_for_ping};
use lv1_scene_fade_utility::osc::OscArg;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Parser)]
#[command(name = "lv1-probe")]
#[command(about = "Phase 1 Waves LV1 protocol discovery probe")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Discover {
        #[arg(long, default_value_t = 6000)]
        timeout_ms: u64,
        #[arg(long)]
        filter_host: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Listen {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long, default_value_t = 6000)]
        timeout_ms: u64,
        #[arg(long, default_value = "logs")]
        log_dir: PathBuf,
        #[arg(long)]
        json: bool,
    },
    SetGain {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        group: i32,
        #[arg(long)]
        channel: i32,
        #[arg(long)]
        gain_db: f64,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Discover { timeout_ms, filter_host, json } => run_discover(timeout_ms, filter_host, json),
        Command::Listen { host, port, timeout_ms, log_dir, json } => run_listen(host, port, timeout_ms, log_dir, json),
        Command::SetGain { host, port, group, channel, gain_db } => run_set_gain(host, port, group, channel, gain_db),
    }
}

fn run_discover(timeout_ms: u64, filter_host: Option<String>, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let entries = discover(DiscoverOptions {
        timeout: Duration::from_millis(timeout_ms),
        filter_host_ip: filter_host,
        ..DiscoverOptions::default()
    })?;

    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        for entry in entries {
            println!("service={} host={:?} port={:?} addresses={:?} source={}", entry.service, entry.host, entry.port, entry.addresses, entry.source);
        }
    }
    Ok(())
}

fn resolve_target(host: Option<String>, port: Option<u16>, timeout_ms: u64) -> Result<(String, u16), Box<dyn std::error::Error>> {
    if let (Some(host), Some(port)) = (host.clone(), port) {
        return Ok((host, port));
    }

    let entries = discover(DiscoverOptions {
        timeout: Duration::from_millis(timeout_ms),
        filter_host_ip: host.clone(),
        ..DiscoverOptions::default()
    })?;
    let entry = entries.first().ok_or("no LV1 targets discovered")?;
    let target_host = host
        .or_else(|| entry.addresses.first().cloned())
        .ok_or("discovered LV1 did not advertise an IPv4 address")?;
    let target_port = port.or(entry.port).ok_or("discovered LV1 did not advertise a TCP port")?;
    Ok((target_host, target_port))
}

fn run_listen(host: Option<String>, port: Option<u16>, timeout_ms: u64, log_dir: PathBuf, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join(format!("lv1-probe-{}.jsonl", unix_timestamp_secs()));
    let mut logger = JsonlLogger::create(&log_path)?;
    let (host, port) = resolve_target(host, port, timeout_ms)?;
    let mut client = Lv1TcpClient::connect(&host, port)?;
    client.register_myfoh("lv1-probe", &uuid::Uuid::new_v4().to_string())?;
    eprintln!("listening on {host}:{port}; writing {}", log_path.display());

    loop {
        for frame in client.read_available()? {
            let msg = decode_frame_payload(&frame)?;
            if let Some((address, args)) = pong_for_ping(&msg) {
                client.send(address, &args)?;
            }
            let entry = entry_for_message("received", &msg, Some(frame.payload.len()), Some(frame.header));
            if !json && matches!(entry.kind, MessageKind::Scene | MessageKind::Fader | MessageKind::Handshake | MessageKind::Keepalive) {
                println!("{:?} {} {:?}", entry.kind, entry.address.as_deref().unwrap_or(""), entry.args);
            }
            if json {
                println!("{}", serde_json::to_string(&entry)?);
            }
            logger.write(entry)?;
        }
    }
}

fn run_set_gain(host: Option<String>, port: Option<u16>, group: i32, channel: i32, gain_db: f64) -> Result<(), Box<dyn std::error::Error>> {
    let (host, port) = resolve_target(host, port, 6000)?;
    let mut client = Lv1TcpClient::connect(&host, port)?;
    client.register_myfoh("lv1-probe", &uuid::Uuid::new_v4().to_string())?;
    client.send(
        "/Set/Track/Out/Gain",
        &[OscArg::Int(group), OscArg::Int(channel), OscArg::Double(gain_db)],
    )?;
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until {
        for frame in client.read_available()? {
            let msg = decode_frame_payload(&frame)?;
            if let Some((address, args)) = pong_for_ping(&msg) {
                client.send(address, &args)?;
            }
            println!("received {} {:?}", msg.address, msg.args);
        }
    }
    Ok(())
}

fn unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
```

- [ ] **Step 2: Run CLI help**

Run: `cargo run -- --help`

Expected: PASS and output includes `discover`, `listen`, and `set-gain`.

- [ ] **Step 3: Run full automated tests**

Run: `cargo test`

Expected: PASS.

- [ ] **Step 4: Commit**

Run:

```bash
git add src/main.rs
git commit -m "feat: add lv1 probe cli"
```

---

### Task 9: Hardware Validation Notes

**Files:**
- Create: `docs/phase-1-hardware-validation.md`

- [ ] **Step 1: Create validation checklist document**

Create `docs/phase-1-hardware-validation.md`:

```markdown
# Phase 1 Hardware Validation

Target: Waves eMotion LV1 software.

## Commands

### Discovery

Run:

```bash
cargo run -- discover --timeout-ms 6000
```

Record whether LV1 appears, which IP is selected, and which TCP port is advertised.

### Listen

Run:

```bash
cargo run -- listen --log-dir logs
```

While listening, recall scenes in LV1 and move safe test faders. Record which OSC addresses appear.

### Gain Send

Pick a non-critical channel. Run:

```bash
cargo run -- set-gain --group 0 --channel 0 --gain-db -20
```

Record whether LV1 moves the fader and whether any echo or notification appears.

## Results

- Discovery finds LV1: not run.
- Handshake succeeds: not run.
- Ping/pong keeps session alive: not run.
- Scene recall messages observed: not run.
- Fader movement messages observed: not run.
- App-sent gain command works: not run.
- App-sent gain echo behavior: not run.

## Notes

Add dated notes here after each hardware run.
```

- [ ] **Step 2: Run formatting and tests**

Run: `cargo fmt && cargo test`

Expected: PASS.

- [ ] **Step 3: Commit**

Run:

```bash
git add docs/phase-1-hardware-validation.md src Cargo.toml Cargo.lock
git commit -m "docs: add phase 1 hardware validation checklist"
```

---

### Task 10: Final Verification

**Files:**
- No code changes expected.

- [ ] **Step 1: Run full verification**

Run: `cargo fmt --check`

Expected: PASS.

Run: `cargo test`

Expected: PASS.

Run: `cargo run -- --help`

Expected: PASS and lists the three commands.

Run: `cargo run -- discover --timeout-ms 1000 --json`

Expected: PASS. It may print `[]` if LV1 is not currently discoverable on the active network.

- [ ] **Step 2: Inspect worktree**

Run: `git status --short`

Expected: clean except for any pre-existing untracked files intentionally left out of commits.

- [ ] **Step 3: Record implementation status**

If hardware validation was not run, leave `docs/phase-1-hardware-validation.md` results as `not run`. If it was run, update the results with observed addresses and log file paths, then commit that documentation update.

Run if docs changed:

```bash
git add docs/phase-1-hardware-validation.md logs
git commit -m "docs: record phase 1 hardware validation results"
```

Expected: commit succeeds if validation docs/logs were intentionally updated.

---

## Self-Review Notes

- Spec coverage: The plan covers custom zDNS discovery, OSC encoding/decoding, LV1 TCP framing, MyFOH handshake helpers, keepalive helpers, structured logging, scene/fader classification, `discover`, `listen`, `set-gain`, automated tests, and hardware validation notes.
- Scope check: The plan excludes fade scheduling, capture workflow, project files, desktop UI, Stream Deck API, and automatic scene fades.
- Type consistency: Shared types are `OscArg`, `OscMessage`, `DiscoveryEntry`, `Lv1Frame`, `Lv1TcpClient`, `MessageKind`, and `ProbeLogEntry`; later tasks use the same names introduced earlier.
- Placeholder scan: The only `not run` text is intentional initial status in the hardware validation document, not an implementation placeholder.

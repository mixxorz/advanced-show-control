//! Waves LV1 OSC-over-TCP framing and client behavior.

use crate::lv1::commands::{Lv1ParameterWrite, Lv1WriteParameter};
use crate::lv1::osc::{OscArg, OscError, OscMessage, decode_packet, encode_message};

type TcpResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

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

pub fn encode_frame(address: &str, args: &[OscArg]) -> Result<Vec<u8>, Lv1TcpError> {
    let payload = encode_message(address, args)?;
    if payload.is_empty() || payload.len() > MAX_FRAME_PAYLOAD {
        return Err(Lv1TcpError::InvalidLength(payload.len()));
    }

    log_osc_message("tx", address);

    let mut frame = Vec::with_capacity(4 + HEADER_LEN + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&DEFAULT_HEADER);
    frame.extend_from_slice(&payload);
    Ok(frame)
}

#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Lv1Frame>, Lv1TcpError> {
        self.buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();

        loop {
            if self.buffer.len() < 4 + HEADER_LEN {
                break;
            }

            let payload_len = u32::from_be_bytes(
                self.buffer[0..4]
                    .try_into()
                    .expect("frame length slice is exactly 4 bytes"),
            ) as usize;
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

pub fn decode_frame_payload(frame: &Lv1Frame) -> Result<OscMessage, Lv1TcpError> {
    let message = decode_packet(&frame.payload)?;
    log_osc_message("rx", &message.address);
    Ok(message)
}

fn log_osc_message(direction: &'static str, address: &str) {
    if is_noisy_osc_log_address(address) {
        return;
    }

    tracing::debug!(
        event = "osc_message",
        direction,
        osc_address = address,
        "{address}"
    );
}

fn is_noisy_osc_log_address(address: &str) -> bool {
    matches!(address, "/ping" | "/pong" | "/Notify/TempoBlink")
}

pub fn encode_parameter_write_batch(writes: &[Lv1ParameterWrite]) -> Result<Vec<u8>, Lv1TcpError> {
    let mut out = Vec::new();

    for write in writes {
        let address = match write.parameter {
            Lv1WriteParameter::FaderDb => "/Set/Track/Out/Gain",
            Lv1WriteParameter::Pan => "/Set/Track/Pan",
            Lv1WriteParameter::Balance => "/Set/Track/Pan/Balance",
            Lv1WriteParameter::Width => "/Set/Track/Pan/Width",
        };

        out.extend_from_slice(&encode_frame(
            address,
            &[
                OscArg::Int(write.group),
                OscArg::Int(write.channel),
                OscArg::Double(write.value),
            ],
        )?);
    }

    Ok(out)
}

pub fn build_myfoh_handshake_batch(device_name: &str, uuid: &str) -> Result<Vec<u8>, Lv1TcpError> {
    let mut out = Vec::new();
    out.extend_from_slice(&encode_frame(
        "/handshake",
        &[OscArg::Int(1), OscArg::Int(-1), OscArg::Int(1)],
    )?);
    out.extend_from_slice(&encode_frame(
        "/device_name",
        &[
            OscArg::String(device_name.to_string()),
            OscArg::String(uuid.to_string()),
        ],
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

pub struct Lv1TcpClient {
    pub(crate) reader: tokio::net::tcp::OwnedReadHalf,
    pub(crate) writer: Option<tokio::net::tcp::OwnedWriteHalf>,
    pub(crate) decoder: FrameDecoder,
}

impl Lv1TcpClient {
    pub async fn connect(host: &str, port: u16) -> std::io::Result<Self> {
        let stream = tokio::net::TcpStream::connect((host, port)).await?;
        stream.set_nodelay(true)?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader,
            writer: Some(writer),
            decoder: FrameDecoder::default(),
        })
    }

    pub async fn register_myfoh(&mut self, device_name: &str, uuid: &str) -> TcpResult<()> {
        let writer = self.writer.as_mut().ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "LV1 TCP writer is not available",
            )) as Box<dyn std::error::Error + Send + Sync>
        })?;

        send_bytes(writer, &build_myfoh_handshake_batch(device_name, uuid)?).await
    }

    pub async fn send(&mut self, address: &str, args: &[OscArg]) -> TcpResult<()> {
        let frame = encode_frame(address, args)?;
        let writer = self.writer.as_mut().ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "LV1 TCP writer is not available",
            )) as Box<dyn std::error::Error + Send + Sync>
        })?;

        send_bytes(writer, &frame).await
    }

    pub async fn read_next(&mut self) -> TcpResult<Vec<Lv1Frame>> {
        read_next_async(&mut self.reader, &mut self.decoder).await
    }

    pub async fn read_available(&mut self) -> TcpResult<Vec<Lv1Frame>> {
        match tokio::time::timeout(std::time::Duration::from_millis(250), self.read_next()).await {
            Ok(result) => result,
            Err(_) => Ok(Vec::new()),
        }
    }
}

async fn send_bytes(writer: &mut tokio::net::tcp::OwnedWriteHalf, bytes: &[u8]) -> TcpResult<()> {
    use tokio::io::AsyncWriteExt;

    writer.write_all(bytes).await?;
    Ok(())
}

pub(crate) async fn read_next_async(
    reader: &mut tokio::net::tcp::OwnedReadHalf,
    decoder: &mut FrameDecoder,
) -> TcpResult<Vec<Lv1Frame>> {
    use tokio::io::AsyncReadExt;

    let mut buf = [0_u8; 8192];
    match reader.read(&mut buf).await {
        Ok(0) => Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "LV1 TCP connection closed",
        ))),
        Ok(size) => Ok(decoder.push(&buf[..size])?),
        Err(err) => Err(Box::new(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::Context;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::registry::{LookupSpan, Registry};

    #[derive(Clone, Default)]
    struct CapturedOscLogs(Arc<Mutex<Vec<CapturedOscLog>>>);

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct CapturedOscLog {
        event: Option<String>,
        direction: Option<String>,
        osc_address: Option<String>,
        message: Option<String>,
    }

    impl<S> Layer<S> for CapturedOscLogs
    where
        S: tracing::Subscriber,
        S: for<'a> LookupSpan<'a>,
    {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut visitor = OscLogVisitor::default();
            event.record(&mut visitor);
            if visitor.log.event.as_deref() == Some("osc_message") {
                self.0.lock().unwrap().push(visitor.log);
            }
        }
    }

    #[derive(Default)]
    struct OscLogVisitor {
        log: CapturedOscLog,
    }

    impl Visit for OscLogVisitor {
        fn record_str(&mut self, field: &Field, value: &str) {
            match field.name() {
                "event" => self.log.event = Some(value.to_string()),
                "direction" => self.log.direction = Some(value.to_string()),
                "osc_address" => self.log.osc_address = Some(value.to_string()),
                "message" => self.log.message = Some(value.to_string()),
                _ => {}
            }
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            let value = format!("{value:?}");
            match field.name() {
                "event" => self.log.event = Some(value.trim_matches('"').to_string()),
                "direction" => self.log.direction = Some(value.trim_matches('"').to_string()),
                "osc_address" => self.log.osc_address = Some(value.trim_matches('"').to_string()),
                "message" => self.log.message = Some(value.trim_matches('"').to_string()),
                _ => {}
            }
        }
    }

    fn capture_osc_logs(run: impl FnOnce()) -> Vec<CapturedOscLog> {
        let captured = CapturedOscLogs::default();
        let logs = captured.0.clone();
        let subscriber = Registry::default().with(captured);
        tracing::subscriber::with_default(subscriber, run);
        logs.lock().unwrap().clone()
    }

    #[test]
    fn encodes_frame_with_payload_length_and_default_header() {
        let frame = encode_frame("/ping", &[OscArg::Int64(123), OscArg::Int(7)]).unwrap();

        let payload_len = u32::from_be_bytes(frame[0..4].try_into().unwrap()) as usize;
        assert_eq!(&frame[4..12], &DEFAULT_HEADER);
        assert_eq!(payload_len, frame.len() - 12);
    }

    #[test]
    fn encode_frame_logs_osc_tx_at_frame_boundary() {
        let logs = capture_osc_logs(|| {
            let _ = encode_frame("/Set/Track/Out/Gain", &[OscArg::Int64(123)]).unwrap();
        });

        assert_eq!(
            logs,
            vec![CapturedOscLog {
                event: Some("osc_message".to_string()),
                direction: Some("tx".to_string()),
                osc_address: Some("/Set/Track/Out/Gain".to_string()),
                message: Some("/Set/Track/Out/Gain".to_string()),
            }]
        );
    }

    #[test]
    fn encode_frame_does_not_log_noisy_osc_tx_messages() {
        for address in ["/ping", "/pong", "/Notify/TempoBlink"] {
            let logs = capture_osc_logs(|| {
                let _ = encode_frame(address, &[OscArg::Int64(123)]).unwrap();
            });

            assert_eq!(logs, Vec::new(), "logged noisy address {address}");
        }
    }

    #[test]
    fn decode_frame_payload_logs_osc_rx_at_frame_boundary() {
        let frame = Lv1Frame {
            header: DEFAULT_HEADER,
            payload: crate::lv1::osc::encode_message("/CurrentScene", &[OscArg::Int64(123)])
                .unwrap(),
        };

        let logs = capture_osc_logs(|| {
            let _ = decode_frame_payload(&frame).unwrap();
        });

        assert_eq!(
            logs,
            vec![CapturedOscLog {
                event: Some("osc_message".to_string()),
                direction: Some("rx".to_string()),
                osc_address: Some("/CurrentScene".to_string()),
                message: Some("/CurrentScene".to_string()),
            }]
        );
    }

    #[test]
    fn decode_frame_payload_does_not_log_noisy_osc_rx_messages() {
        for address in ["/ping", "/pong", "/Notify/TempoBlink"] {
            let frame = Lv1Frame {
                header: DEFAULT_HEADER,
                payload: crate::lv1::osc::encode_message(address, &[OscArg::Int64(123)]).unwrap(),
            };

            let logs = capture_osc_logs(|| {
                let _ = decode_frame_payload(&frame).unwrap();
            });

            assert_eq!(logs, Vec::new(), "logged noisy address {address}");
        }
    }

    #[test]
    fn rejects_encoded_payloads_that_exceed_max_frame_payload() {
        let err = encode_frame("/blob", &[OscArg::Blob(vec![0; MAX_FRAME_PAYLOAD])]).unwrap_err();

        assert!(matches!(
            err,
            Lv1TcpError::InvalidLength(length) if length > MAX_FRAME_PAYLOAD
        ));
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
        assert_eq!(
            decode_frame_payload(&frames[0]).unwrap().address,
            "/handshake"
        );
    }

    #[test]
    fn rejects_impossible_lengths() {
        let mut decoder = FrameDecoder::default();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&((MAX_FRAME_PAYLOAD as u32) + 1).to_be_bytes());
        bytes.extend_from_slice(&DEFAULT_HEADER);

        assert!(matches!(
            decoder.push(&bytes),
            Err(Lv1TcpError::InvalidLength(_))
        ));
    }

    #[test]
    fn rejects_zero_length_payloads() {
        let mut decoder = FrameDecoder::default();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&DEFAULT_HEADER);

        assert!(matches!(
            decoder.push(&bytes),
            Err(Lv1TcpError::InvalidLength(0))
        ));
    }

    #[test]
    fn decodes_frame_from_byte_by_byte_pushes() {
        let bytes = encode_frame("/meter", &[OscArg::Float(0.5)]).unwrap();
        let mut decoder = FrameDecoder::default();
        let mut frames = Vec::new();

        for byte in bytes {
            frames.extend(decoder.push(&[byte]).unwrap());
        }

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].header, DEFAULT_HEADER);
        assert_eq!(decode_frame_payload(&frames[0]).unwrap().address, "/meter");
    }

    #[test]
    fn decodes_multiple_complete_frames_from_one_push() {
        let first = encode_frame("/first", &[OscArg::Int(1)]).unwrap();
        let second = encode_frame("/second", &[OscArg::Int(2)]).unwrap();
        let mut bytes = first;
        bytes.extend_from_slice(&second);
        let mut decoder = FrameDecoder::default();

        let frames = decoder.push(&bytes).unwrap();

        assert_eq!(frames.len(), 2);
        assert_eq!(decode_frame_payload(&frames[0]).unwrap().address, "/first");
        assert_eq!(decode_frame_payload(&frames[1]).unwrap().address, "/second");
    }

    #[test]
    fn encoded_frames_round_trip_through_decoder() {
        let args = [OscArg::String("scene-a".to_owned()), OscArg::Int64(42)];
        let bytes = encode_frame("/scene/fade", &args).unwrap();
        let mut decoder = FrameDecoder::default();

        let frames = decoder.push(&bytes).unwrap();
        let message = decode_frame_payload(&frames[0]).unwrap();

        assert_eq!(frames.len(), 1);
        assert_eq!(message.address, "/scene/fade");
        assert_eq!(message.args, args);
    }

    #[test]
    fn builds_myfoh_handshake_batch() {
        let bytes = build_myfoh_handshake_batch("lv1-probe", "uuid-1").unwrap();
        let mut decoder = FrameDecoder::default();
        let frames = decoder.push(&bytes).unwrap();

        assert_eq!(frames.len(), 2);
        assert_eq!(
            decode_frame_payload(&frames[0]).unwrap().address,
            "/handshake"
        );
        assert_eq!(
            decode_frame_payload(&frames[1]).unwrap().address,
            "/device_name"
        );
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

    #[tokio::test]
    async fn client_registers_sends_and_reads_available_frames() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;
        use std::time::Duration;

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_millis(500)))
                .unwrap();
            let mut decoder = FrameDecoder::default();
            let mut frames = Vec::new();
            let mut buf = [0_u8; 1024];

            while frames.len() < 3 {
                let size = stream.read(&mut buf).unwrap();
                frames.extend(decoder.push(&buf[..size]).unwrap());
            }

            stream
                .write_all(&encode_frame("/ping", &[OscArg::Int64(123)]).unwrap())
                .unwrap();
            frames
                .into_iter()
                .map(|frame| decode_frame_payload(&frame).unwrap())
                .collect::<Vec<_>>()
        });

        let mut client = Lv1TcpClient::connect("127.0.0.1", port).await.unwrap();
        client.register_myfoh("lv1-probe", "uuid-1").await.unwrap();
        client.send("/custom", &[OscArg::Int(5)]).await.unwrap();

        let mut frames = Vec::new();
        for _ in 0..10 {
            frames.extend(client.read_available().await.unwrap());
            if !frames.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        let received = server.join().unwrap();
        assert_eq!(received[0].address, "/handshake");
        assert_eq!(received[1].address, "/device_name");
        assert_eq!(received[2].address, "/custom");
        assert_eq!(decode_frame_payload(&frames[0]).unwrap().address, "/ping");
    }

    #[tokio::test]
    async fn client_read_available_errors_when_peer_closes_connection() {
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
        });

        let mut client = Lv1TcpClient::connect("127.0.0.1", port).await.unwrap();
        server.join().unwrap();

        let err = client.read_available().await.unwrap_err();
        let io_err = err.downcast_ref::<std::io::Error>().unwrap();

        assert_eq!(io_err.kind(), std::io::ErrorKind::UnexpectedEof);
        assert_eq!(io_err.to_string(), "LV1 TCP connection closed");
    }

    #[test]
    fn encodes_parameter_write_batch_in_order() {
        let bytes = encode_parameter_write_batch(&[
            Lv1ParameterWrite {
                group: 0,
                channel: 1,
                parameter: Lv1WriteParameter::FaderDb,
                value: -12.5,
            },
            Lv1ParameterWrite {
                group: 2,
                channel: 3,
                parameter: Lv1WriteParameter::Pan,
                value: 15.0,
            },
            Lv1ParameterWrite {
                group: 4,
                channel: 5,
                parameter: Lv1WriteParameter::Balance,
                value: -25.0,
            },
            Lv1ParameterWrite {
                group: 6,
                channel: 7,
                parameter: Lv1WriteParameter::Width,
                value: 0.75,
            },
        ])
        .unwrap();

        let mut decoder = FrameDecoder::default();
        let frames = decoder.push(&bytes).unwrap();
        let messages = frames
            .iter()
            .map(decode_frame_payload)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(messages[0].address, "/Set/Track/Out/Gain");
        assert_eq!(
            messages[0].args,
            vec![OscArg::Int(0), OscArg::Int(1), OscArg::Double(-12.5)]
        );
        assert_eq!(messages[1].address, "/Set/Track/Pan");
        assert_eq!(
            messages[1].args,
            vec![OscArg::Int(2), OscArg::Int(3), OscArg::Double(15.0)]
        );
        assert_eq!(messages[2].address, "/Set/Track/Pan/Balance");
        assert_eq!(
            messages[2].args,
            vec![OscArg::Int(4), OscArg::Int(5), OscArg::Double(-25.0)]
        );
        assert_eq!(messages[3].address, "/Set/Track/Pan/Width");
        assert_eq!(
            messages[3].args,
            vec![OscArg::Int(6), OscArg::Int(7), OscArg::Double(0.75)]
        );
    }

    #[test]
    fn empty_parameter_write_batch_encodes_to_empty_buffer() {
        assert_eq!(encode_parameter_write_batch(&[]).unwrap(), Vec::<u8>::new());
    }
}

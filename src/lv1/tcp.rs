//! Waves LV1 OSC-over-TCP framing and client behavior.

use crate::osc::{decode_packet, encode_message, OscArg, OscError, OscMessage};

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
    Ok(decode_packet(&frame.payload)?)
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
    stream: std::net::TcpStream,
    decoder: FrameDecoder,
}

impl Lv1TcpClient {
    pub fn connect(host: &str, port: u16) -> std::io::Result<Self> {
        let stream = std::net::TcpStream::connect((host, port))?;
        stream.set_read_timeout(Some(std::time::Duration::from_millis(250)))?;
        Ok(Self {
            stream,
            decoder: FrameDecoder::default(),
        })
    }

    pub fn register_myfoh(
        &mut self,
        device_name: &str,
        uuid: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;

        let batch = build_myfoh_handshake_batch(device_name, uuid)?;
        self.stream.write_all(&batch)?;
        Ok(())
    }

    pub fn send(
        &mut self,
        address: &str,
        args: &[OscArg],
    ) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;

        let frame = encode_frame(address, args)?;
        self.stream.write_all(&frame)?;
        Ok(())
    }

    pub fn read_available(&mut self) -> Result<Vec<Lv1Frame>, Box<dyn std::error::Error>> {
        use std::io::Read;

        let mut buf = [0_u8; 8192];
        match self.stream.read(&mut buf) {
            Ok(0) => Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "LV1 TCP connection closed",
            ))),
            Ok(size) => Ok(self.decoder.push(&buf[..size])?),
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                Ok(Vec::new())
            }
            Err(err) => Err(Box::new(err)),
        }
    }
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

    #[test]
    fn client_registers_sends_and_reads_available_frames() {
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

        let mut client = Lv1TcpClient::connect("127.0.0.1", port).unwrap();
        client.register_myfoh("lv1-probe", "uuid-1").unwrap();
        client.send("/custom", &[OscArg::Int(5)]).unwrap();

        let mut frames = Vec::new();
        for _ in 0..10 {
            frames.extend(client.read_available().unwrap());
            if !frames.is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }

        let received = server.join().unwrap();
        assert_eq!(received[0].address, "/handshake");
        assert_eq!(received[1].address, "/device_name");
        assert_eq!(received[2].address, "/custom");
        assert_eq!(decode_frame_payload(&frames[0]).unwrap().address, "/ping");
    }

    #[test]
    fn client_read_available_errors_when_peer_closes_connection() {
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
        });

        let mut client = Lv1TcpClient::connect("127.0.0.1", port).unwrap();
        server.join().unwrap();

        let err = client.read_available().unwrap_err();
        let io_err = err.downcast_ref::<std::io::Error>().unwrap();

        assert_eq!(io_err.kind(), std::io::ErrorKind::UnexpectedEof);
        assert_eq!(io_err.to_string(), "LV1 TCP connection closed");
    }
}

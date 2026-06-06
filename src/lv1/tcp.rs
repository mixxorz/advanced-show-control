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
}

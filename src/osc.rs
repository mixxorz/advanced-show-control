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
    #[error("OSC string contains embedded null byte")]
    EmbeddedNul,
    #[error("OSC string is not valid UTF-8")]
    InvalidUtf8,
    #[error("OSC blob length {0} exceeds i32::MAX")]
    BlobTooLarge(usize),
    #[error("OSC blob length {0} is negative")]
    NegativeBlobLength(i32),
    #[error("OSC string/blob padding bytes must be zero")]
    NonZeroPadding,
    #[error("OSC packet has {0} trailing bytes after message")]
    TrailingData(usize),
    #[error("OSC packet length {0} is not a multiple of 4")]
    InvalidPaddedLength(usize),
}

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
    let value = std::str::from_utf8(&bytes[start..end])
        .map_err(|_| OscError::InvalidUtf8)?
        .to_string();
    let raw_len = end - start + 1;
    let padded_len = raw_len + pad_to_4(raw_len);
    if start + padded_len > bytes.len() {
        return Err(OscError::UnexpectedEof("string padding"));
    }
    for &b in &bytes[start + raw_len..start + padded_len] {
        if b != 0 {
            return Err(OscError::NonZeroPadding);
        }
    }
    *offset = start + padded_len;
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
    if address.contains('\0') {
        return Err(OscError::EmbeddedNul);
    }
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
            OscArg::String(value) => {
                if value.contains('\0') {
                    return Err(OscError::EmbeddedNul);
                }
                encode_string(value, &mut out);
            }
            OscArg::Blob(value) => {
                if value.len() > i32::MAX as usize {
                    return Err(OscError::BlobTooLarge(value.len()));
                }
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
    if bytes.len() % 4 != 0 {
        return Err(OscError::InvalidPaddedLength(bytes.len()));
    }
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
                let len_i32 = i32::from_be_bytes(take(bytes, &mut offset, "blob length")?);
                let len = usize::try_from(len_i32)
                    .map_err(|_| OscError::NegativeBlobLength(len_i32))?;
                if offset + len > bytes.len() {
                    return Err(OscError::UnexpectedEof("blob"));
                }
                let padded_len = len + pad_to_4(len);
                if offset + padded_len > bytes.len() {
                    return Err(OscError::UnexpectedEof("blob padding"));
                }
                for &b in &bytes[offset + len..offset + padded_len] {
                    if b != 0 {
                        return Err(OscError::NonZeroPadding);
                    }
                }
                let value = bytes[offset..offset + len].to_vec();
                offset += padded_len;
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

    if offset != bytes.len() {
        return Err(OscError::TrailingData(bytes.len() - offset));
    }

    Ok(OscMessage { address, args })
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

    #[test]
    fn encoder_rejects_embedded_nul_in_address() {
        assert_eq!(
            encode_message("/bad\0address", &[]),
            Err(OscError::EmbeddedNul)
        );
    }

    #[test]
    fn encoder_rejects_embedded_nul_in_string_arg() {
        assert_eq!(
            encode_message("/s", &[OscArg::String("bad\0".to_string())]),
            Err(OscError::EmbeddedNul)
        );
    }

    #[test]
    fn encoder_writes_zero_padding_for_strings_and_blobs() {
        let bytes = encode_message(
            "/pad",
            &[OscArg::String("abc".to_string()), OscArg::Blob(vec![1, 2, 3])],
        )
        .unwrap();
        // address "/pad\0" + 3 zero pad bytes (positions 4-7)
        assert_eq!(&bytes[4..8], &[0, 0, 0, 0]);
        // the trailing byte is the single zero pad for the 3-byte blob
        assert_eq!(*bytes.last().unwrap(), 0);
    }

    #[test]
    fn decoder_rejects_empty_packet() {
        assert!(decode_packet(b"").is_err());
    }

    #[test]
    fn decoder_rejects_under_padded_string() {
        assert!(decode_packet(b"\0").is_err());
    }

    #[test]
    fn decoder_rejects_unsupported_type_tag() {
        let packet = [b'/', b'x', 0, 0, b',', b'x', 0, 0];
        assert_eq!(
            decode_packet(&packet),
            Err(OscError::UnsupportedType('x'))
        );
    }

    #[test]
    fn decoder_rejects_missing_comma_type_tag() {
        let packet = [b'/', b'x', 0, 0, b'i', b'f', 0, 0];
        assert_eq!(decode_packet(&packet), Err(OscError::InvalidTypeTag));
    }

    #[test]
    fn decoder_rejects_negative_blob_length() {
        let packet = [
            b'/', b'b', 0, 0,
            b',', b'b', 0, 0,
            0xFF, 0xFF, 0xFF, 0xFF,
        ];
        assert_eq!(
            decode_packet(&packet),
            Err(OscError::NegativeBlobLength(-1))
        );
    }

    #[test]
    fn decoder_rejects_truncated_blob() {
        // claims 5 bytes of data but only 2 follow; total length is not a multiple of 4
        let packet = [
            b'/', b'b', 0, 0,
            b',', b'b', 0, 0,
            0, 0, 0, 5,
            1, 2,
        ];
        assert!(decode_packet(&packet).is_err());
    }
}

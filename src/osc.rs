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
